//! 对应 `rtg/TerrainGenerator.php`。
//!
//! 一个区块的完整流程：高度图 → 逐列选 biome（含海岸抖动）→ 地表涂抹 →
//! 河流绿洲叠加 → 基岩 → 洞穴 → 峡谷。fitted primer 同时写入装饰前高度缓存。
//!
//! 注意 `RtgWorld` 在这里有两份：地表层要拿共享的 `rand()`，而
//! `LandscapeGenerator` 自己持有一份。两份都由种子决定、只读噪声，
//! 因此数值上完全等价。

use std::collections::{HashMap, VecDeque};

use crate::level::generator::realistic_terrain::biome::BiomeProvider;
use crate::level::generator::realistic_terrain::carver::CarverWorld;
use crate::level::generator::realistic_terrain::carver::caves::Caves;
use crate::level::generator::realistic_terrain::carver::ravines::Ravines;
use crate::level::generator::realistic_terrain::chunk_primer::ChunkPrimer;
use crate::level::generator::realistic_terrain::landscape::LandscapeGenerator;
use crate::level::generator::realistic_terrain::math::{JavaRandom, round};
use crate::level::generator::realistic_terrain::structure::village::{
    TerrainHeightProvider, VillageMapGenerator, placer,
};
use crate::level::generator::realistic_terrain::surface;
use crate::level::generator::realistic_terrain::terrain::profiles;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::world::GenWorld;

/// 这些 biome 上会再叠一层 `SurfaceRiverOasis`。
const OASIS_BIOMES: [i32; 9] = [2, 17, 130, 37, 38, 39, 165, 166, 167];

/// `TerrainSettings::$caveMode`：只影响洞穴密度/概率的默认值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaveMode {
    Vanilla,
    Rtg,
}

/// 对应 `rtg/TerrainSettings.php`。
#[derive(Clone, Copy, Debug)]
pub struct TerrainSettings {
    pub cave_mode: CaveMode,
    pub cave_chance: i32,
    pub cave_density: i32,
    pub ravine_chance: i32,
    pub use_caves: bool,
    pub use_ravines: bool,
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self::with_cave_mode(CaveMode::Vanilla)
    }
}

impl TerrainSettings {
    pub fn with_cave_mode(cave_mode: CaveMode) -> Self {
        let vanilla = cave_mode == CaveMode::Vanilla;
        Self {
            cave_mode,
            cave_chance: if vanilla { 7 } else { 10 },
            cave_density: if vanilla { 15 } else { 7 },
            ravine_chance: 120,
            use_caves: true,
            use_ravines: true,
        }
    }

    /// 关掉两种 carver，用来单独比地表层（PHP 的 `useCaves/useRavines => false`）。
    pub fn without_carvers(mut self) -> Self {
        self.use_caves = false;
        self.use_ravines = false;
        self
    }
}

pub struct TerrainGenerator {
    seed: i64,
    settings: TerrainSettings,
    world: RtgWorld,
    landscapes: LandscapeGenerator,
    surfaces: HashMap<i32, surface::Surface>,
    carver_world: CarverWorld,
    caves: Option<Caves>,
    ravines: Option<Ravines>,
    // 装饰前的 fitted 地形高度，村庄放置不能扫描已经搭好的屋顶。
    height_maps: HashMap<(i32, i32), [u16; 256]>,
    height_order: VecDeque<(i32, i32)>,
    villages: VillageMapGenerator,
}

impl Clone for TerrainGenerator {
    /// `level.rs::set_generator` 会给每个 region 复制一份。
    /// 同 seed 重建即可：LandscapeGenerator 的缓存是可丢弃的，数值等价。
    fn clone(&self) -> Self {
        Self::with_settings(self.seed, self.settings)
    }
}

impl TerrainGenerator {
    pub fn new(seed: i64) -> Self {
        Self::with_settings(seed, TerrainSettings::default())
    }

    pub fn with_settings(seed: i64, settings: TerrainSettings) -> Self {
        Self {
            seed,
            settings,
            world: RtgWorld::new(seed),
            landscapes: LandscapeGenerator::new(RtgWorld::new(seed), BiomeProvider::new(seed), 16),
            surfaces: HashMap::new(),
            carver_world: CarverWorld::new(seed),
            caves: settings
                .use_caves
                .then(|| Caves::new(settings.cave_chance, settings.cave_density)),
            ravines: settings
                .use_ravines
                .then(|| Ravines::new(settings.ravine_chance)),
            height_maps: HashMap::new(),
            height_order: VecDeque::new(),
            villages: VillageMapGenerator::new(seed),
        }
    }

    pub fn world(&self) -> &RtgWorld {
        &self.world
    }

    /// 原始 256 高地形，逐位对齐 PHP（黄金值测试用）。
    pub fn generate(&mut self, chunk_x: i32, chunk_z: i32) -> ChunkPrimer {
        self.build(chunk_x, chunk_z, false).0
    }

    /// 给 `Generator::Realistic` 用：高度压到 128 以内（MCPE 0.14.3 的世界高度），
    /// 并带回每列最终 biome（海岸抖动之后的值，`ChunkPrimer` 里不存）。
    pub fn generate_fitted(&mut self, chunk_x: i32, chunk_z: i32) -> (ChunkPrimer, [[u8; 16]; 16]) {
        self.build(chunk_x, chunk_z, true)
    }

    /// 对应 `RTGWorld::getChunkSeed`：装饰阶段的随机源种子。
    pub fn chunk_seed(&mut self, chunk_x: i32, chunk_z: i32) -> i64 {
        self.world.get_chunk_seed(chunk_x as i64, chunk_z as i64)
    }

    /// 装饰阶段要用 simplex 噪声明密度，但只有 `&self` 就够。
    pub fn rtg_world(&self) -> &RtgWorld {
        &self.world
    }

    /// 对应 `TerrainGenerator::getTerrainHeight`：返回装饰前最高实心方块之上的 y。
    /// Rust 世界只有 128 高，因此缓存来自 fitted primer，而不是 PHP 的原始 256 高 primer。
    pub fn terrain_height(&mut self, x: i32, z: i32) -> i32 {
        let chunk_x = x >> 4;
        let chunk_z = z >> 4;
        let index = ((x & 15) * 16 + (z & 15)) as usize;
        if let Some(map) = self.height_maps.get(&(chunk_x, chunk_z)) {
            return map[index] as i32;
        }

        // 异步 worker 可能缺少邻块快照；重建不能改变 population 前后的共享 RNG 序列。
        let state = self.world.rand().state();
        let _ = self.generate_fitted(chunk_x, chunk_z);
        self.world.rand().restore(state);
        self.height_maps[&(chunk_x, chunk_z)][index] as i32
    }

    fn cache_terrain_height_map(&mut self, chunk_x: i32, chunk_z: i32, map: [u16; 256]) {
        const CAPACITY: usize = 128;
        let key = (chunk_x, chunk_z);
        if let Some(position) = self.height_order.iter().position(|&cached| cached == key) {
            self.height_order.remove(position);
        }
        self.height_maps.insert(key, map);
        self.height_order.push_back(key);
        if self.height_maps.len() > CAPACITY {
            if let Some(oldest) = self.height_order.pop_front() {
                self.height_maps.remove(&oldest);
            }
        }
    }

    pub fn populate_villages(
        &mut self,
        world: &mut GenWorld,
        random: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
    ) -> bool {
        let starts = self.villages.starts_for_population_chunk(chunk_x, chunk_z);
        placer::generate(world, self, random, &starts, chunk_x, chunk_z)
    }

    /// 对应 `PopulationStage::populate`：先村庄结构，再执行已移植的 biome 装饰。
    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        crate::level::generator::realistic_terrain::populate::populate_chunk(
            self, world, chunk_x, chunk_z,
        );
    }

    fn build(&mut self, chunk_x: i32, chunk_z: i32, fit: bool) -> (ChunkPrimer, [[u8; 16]; 16]) {
        {
            let mut rand = self.world.rand();
            rand.set_seed(
                (chunk_x as i64)
                    .wrapping_mul(341873128712)
                    .wrapping_add((chunk_z as i64).wrapping_mul(132897987541)),
            );
        }

        let landscape = self.landscapes.get(chunk_x, chunk_z);
        let fitted: Vec<f64>;
        let heights: &[f64] = if fit {
            fitted = landscape
                .noise
                .iter()
                .map(|&height| fit_world_height(height))
                .collect();
            &fitted
        } else {
            &landscape.noise[..]
        };
        let mut primer = ChunkPrimer::new(heights);
        let simplex = self.world.simplex_instance(0);
        let mut biomes = [[0u8; 16]; 16];

        for x in 0..16_i32 {
            for z in 0..16_i32 {
                let world_x = chunk_x * 16 + x;
                let world_z = chunk_z * 16 + z;
                let index = (x * 16 + z) as usize;
                let mut biome = landscape.biome[index];
                biomes[x as usize][z as usize] = biome as u8;

                // 海岸抖动：只有允许"渗入"的 biome 会取邻居，且邻居要允许"渗出"。
                if profiles::get(biome).is_some_and(|profile| profile.bleed_in) {
                    let delta = simplex.multi_eval2d(world_x as f64, world_z as f64, false);
                    let px = round(world_x as f64 + delta[0] * 32.0) as i32;
                    let pz = round(world_z as f64 + delta[1] * 32.0) as i32;
                    let other = landscape.biome[((px & 15) * 16 + (pz & 15)) as usize];
                    if profiles::get(other).is_some_and(|profile| profile.bleed_out) {
                        biome = other;
                    }
                }

                let river = if profiles::get(biome).is_some_and(|profile| profile.allow_rivers) {
                    landscape.river[index]
                } else {
                    0.0
                };

                let surface = self
                    .surfaces
                    .entry(biome)
                    .or_insert_with(|| surface::create(biome));
                surface.paint_terrain(
                    &mut primer,
                    world_x,
                    world_z,
                    x,
                    z,
                    -1,
                    &self.world,
                    heights,
                    river,
                );

                if OASIS_BIOMES.contains(&biome) {
                    surface::paint_river_oasis(
                        &mut primer,
                        world_x,
                        world_z,
                        x,
                        z,
                        -1,
                        &self.world,
                        river,
                    );
                }

                {
                    let mut rand = self.world.rand();
                    let mut y = 9;
                    while y >= 0 {
                        if y <= rand.next_int_bound(5) {
                            primer.set_block_state(x, y, z, 112);
                        }
                        y -= 1;
                    }
                }
            }
        }

        if let Some(caves) = self.caves.as_mut() {
            caves.generate(&self.carver_world, chunk_x, chunk_z, &mut primer);
        }
        if let Some(ravines) = self.ravines.as_mut() {
            ravines.generate(&self.carver_world, chunk_x, chunk_z, &mut primer);
        }

        if fit {
            self.cache_terrain_height_map(chunk_x, chunk_z, primer.terrain_heights());
        }
        (primer, biomes)
    }
}

impl TerrainHeightProvider for TerrainGenerator {
    fn terrain_height(&mut self, x: i32, z: i32) -> i32 {
        TerrainGenerator::terrain_height(self, x, z)
    }
}

/// MCPE 0.14.3 的区块只有 128 高，而 RTG 的原始高度实测上限 175（`height_stats` 扫 950k 列）。
/// 96 以下原样保留（海平面 63、河流、丘陵都不受影响），以上线性压到 127，
/// 这样山顶是被压扁而不是被硬切成平台。超过 96 的列只占 5%。
pub fn fit_world_height(height: f64) -> f64 {
    const SOFT_LIMIT: f64 = 96.0;
    const RAW_LIMIT: f64 = 175.0;
    const MAX_Y: f64 = 127.0;
    if height <= SOFT_LIMIT {
        height
    } else {
        SOFT_LIMIT + (height - SOFT_LIMIT) * (MAX_Y - SOFT_LIMIT) / (RAW_LIMIT - SOFT_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_height_uses_fitted_primer() {
        let mut generator = TerrainGenerator::new(20261115);
        let (primer, _) = generator.generate_fitted(-2, 3);
        let expected = primer.terrain_heights();
        for (x, z) in [(-32, 48), (-31, 49), (-17, 63)] {
            let index = ((x & 15) * 16 + (z & 15)) as usize;
            assert_eq!(generator.terrain_height(x, z), expected[index] as i32);
        }
    }

    #[test]
    fn terrain_height_cache_refreshes_before_eviction() {
        let mut generator = TerrainGenerator::new(1);
        for chunk_x in 0..129 {
            let mut map = [0u16; 256];
            map[0] = chunk_x as u16;
            generator.cache_terrain_height_map(chunk_x, 0, map);
        }
        assert_eq!(generator.height_maps.len(), 128);
        assert!(!generator.height_maps.contains_key(&(0, 0)));

        generator.cache_terrain_height_map(1, 0, [7u16; 256]);
        generator.cache_terrain_height_map(129, 0, [9u16; 256]);
        assert!(generator.height_maps.contains_key(&(1, 0)));
        assert!(!generator.height_maps.contains_key(&(2, 0)));
    }

    #[test]
    fn missing_height_probe_preserves_shared_random() {
        let mut expected_generator = TerrainGenerator::new(9981);
        let (primer, _) = expected_generator.generate_fitted(4, -2);
        let expected = primer.terrain_heights();

        let mut generator = TerrainGenerator::new(9981);
        let _ = generator.world.rand().next_int();
        let state = generator.world.rand().state();
        assert_eq!(
            generator.terrain_height(65, -31),
            expected[1 * 16 + 1] as i32
        );
        assert_eq!(generator.world.rand().state(), state);
    }

    /// PHP `TerrainGenerator($seed, ['useCaves' => false, 'useRavines' => false])`
    /// 对同样区块的 primer 做 32 位 FNV-1a 的结果。
    #[test]
    fn matches_php_fingerprint() {
        let mut generator =
            TerrainGenerator::with_settings(1234, TerrainSettings::default().without_carvers());
        let chunks: [(i32, i32); 39] = [
            (-200, 200),
            (-48, -48),
            (-48, -24),
            (-48, 0),
            (-48, 24),
            (-48, 48),
            (-40, -40),
            (-24, -48),
            (-24, -24),
            (-24, 0),
            (-24, 24),
            (-24, 48),
            (-10, -10),
            (-10, 10),
            (-1, -1),
            (0, -48),
            (0, -24),
            (0, 0),
            (0, 1),
            (0, 24),
            (0, 48),
            (0, 100),
            (1, 0),
            (3, -2),
            (10, -10),
            (10, 10),
            (24, -48),
            (24, -24),
            (24, 0),
            (24, 24),
            (24, 48),
            (40, 40),
            (48, -48),
            (48, -24),
            (48, 0),
            (48, 24),
            (48, 48),
            (100, 0),
            (200, -200),
        ];
        let expected: [u32; 39] = [
            952833041, 2680174925, 806896966, 968471458, 1286190435, 782492258, 3964130451,
            3950884437, 880218031, 268064392, 3038004642, 1449756384, 625600904, 3977660711,
            3698338766, 1114506737, 3819848089, 3457724623, 2613028021, 2245213409, 1700189468,
            1531241435, 2186180714, 3252872571, 449277940, 2076335047, 171663862, 2207676273,
            2189286604, 1048653132, 3254394281, 3535134249, 1126775824, 1396792447, 161797304,
            751045493, 1441438074, 869183331, 2380734055,
        ];

        for (i, &(chunk_x, chunk_z)) in chunks.iter().enumerate() {
            let primer = generator.generate(chunk_x, chunk_z);
            assert_eq!(
                primer.fingerprint(),
                expected[i],
                "chunk {chunk_x} {chunk_z} 与 PHP 不一致"
            );
        }
    }

    /// 广域抽样：±240 区块内 60 个点，覆盖 19 种 biome（含白桦林/森林丘陵/针叶林/
    /// 冰原/海洋族/阳光草原等）。期望值同样来自 PHP `TerrainGenerator`。
    #[test]
    fn matches_php_fingerprint_wide() {
        let mut generator =
            TerrainGenerator::with_settings(1234, TerrainSettings::default().without_carvers());
        let chunks: [(i32, i32); 60] = [
            (-240, -198),
            (96, 96),
            (-54, -96),
            (-204, 198),
            (132, 6),
            (-18, -186),
            (-168, 108),
            (168, -84),
            (18, 210),
            (-132, 18),
            (204, -174),
            (54, 120),
            (-96, -72),
            (240, 222),
            (90, 30),
            (-60, -162),
            (-210, 132),
            (126, -60),
            (-24, 234),
            (-174, 42),
            (162, -150),
            (12, 144),
            (-138, -48),
            (198, -240),
            (48, 54),
            (-102, -138),
            (234, 156),
            (84, -36),
            (-66, -228),
            (-216, 66),
            (120, -126),
            (-30, 168),
            (-180, -24),
            (156, -216),
            (6, 78),
            (-144, -114),
            (192, 180),
            (42, -12),
            (-108, -204),
            (228, 90),
            (78, -102),
            (-72, 192),
            (-222, 0),
            (114, -192),
            (-36, 102),
            (-186, -90),
            (150, 204),
            (0, 12),
            (-150, -180),
            (186, 114),
            (36, -78),
            (-114, 216),
            (222, 24),
            (72, -168),
            (-78, 126),
            (-228, -66),
            (108, 228),
            (-42, 36),
            (-192, -156),
            (144, 138),
        ];
        let expected: [u32; 60] = [
            4215526530, 2899922303, 2059458943, 4034404751, 4138887468, 2247722533, 4174868284,
            3495881291, 1793327108, 3919949833, 869189238, 1327225072, 415803436, 259838863,
            2346035108, 423472624, 2698673210, 305842401, 1086527378, 2151747333, 76499657,
            2832669585, 1722458484, 2150125655, 3949562162, 3219157709, 2516566455, 2418664752,
            2147921166, 2246748624, 737483720, 4124453192, 44553856, 3703257257, 4129671658,
            693027214, 4035077438, 3116272887, 339102161, 2306368914, 3292369323, 2920705364,
            2200727747, 99794720, 2502756452, 2333320832, 1157199783, 2687255991, 1400608943,
            254782452, 30105722, 3267023467, 3964493262, 189092289, 3585543982, 1015456593,
            1171350600, 3545446322, 3890223498, 1540758330,
        ];

        for (i, &(chunk_x, chunk_z)) in chunks.iter().enumerate() {
            let primer = generator.generate(chunk_x, chunk_z);
            assert_eq!(
                primer.fingerprint(),
                expected[i],
                "chunk {chunk_x} {chunk_z} 与 PHP 不一致"
            );
        }
    }

    /// 每个 biome 取一块：扫描 ±400 区块后，为 52 种 biome 各挑一个首次出现的区块。
    /// 期望值来自 PHP `TerrainGenerator` 的同区块 primer。
    #[test]
    fn matches_php_fingerprint_biomes() {
        let mut generator =
            TerrainGenerator::with_settings(1234, TerrainSettings::default().without_carvers());
        let chunks: [(i32, i32); 52] = [
            (-400, -260),
            (-400, -324),
            (-364, 212),
            (-400, -380),
            (-400, -316),
            (-400, 12),
            (-400, -396),
            (-400, -400),
            (-400, -24),
            (-396, -48),
            (-392, -64),
            (-312, -128),
            (-308, -132),
            (-400, -268),
            (-364, 212),
            (-400, 4),
            (-384, 28),
            (236, 400),
            (352, 44),
            (348, 12),
            (-400, -252),
            (-376, 376),
            (-400, 232),
            (-400, -280),
            (-400, -284),
            (-400, -332),
            (-400, 236),
            (-400, 236),
            (-272, -116),
            (-272, -116),
            (-396, -32),
            (-340, 208),
            (-340, 212),
            (96, 308),
            (108, 324),
            (116, 316),
            (-396, -40),
            (-280, 208),
            (-364, -280),
            (-372, -288),
            (-348, -24),
            (-388, -364),
            (-376, -76),
            (-364, -300),
            (-400, 88),
            (-384, -364),
            (-288, 328),
            (-252, -132),
            (-240, -136),
            (-340, 56),
            (-72, -332),
            (-120, 104),
        ];
        let expected: [u32; 52] = [
            2579354205, 1419096363, 1894578987, 727053319, 1760390161, 2928615407, 1641275209,
            509615515, 996860939, 4135460520, 281206167, 3088468424, 1734288999, 1228321465,
            1894578987, 1974924300, 1238646237, 1733556096, 2659026089, 3994412316, 1959301299,
            3945195425, 2552658539, 485877123, 1247112309, 3532077534, 160915698, 160915698,
            3773792464, 3773792464, 2910905665, 3215493080, 38826901, 3316740936, 4185714177,
            240413361, 1645322934, 1227889603, 884511398, 3536481746, 1998192659, 2814572630,
            1977303077, 168929701, 2733828565, 1126282773, 3693245387, 4122414568, 2100975976,
            651284780, 1696806092, 3146919451,
        ];

        for (i, &(chunk_x, chunk_z)) in chunks.iter().enumerate() {
            let primer = generator.generate(chunk_x, chunk_z);
            assert_eq!(
                primer.fingerprint(),
                expected[i],
                "chunk {chunk_x} {chunk_z} 与 PHP 不一致"
            );
        }
    }

    /// 带洞穴/峡谷的黄金值：期望值来自 PHP `new TerrainGenerator(1234)`（默认开启 carver）。
    /// 同时比对指纹和空气方块数——后者对"挖多了/挖少了"更敏感。
    #[test]
    fn matches_php_fingerprint_carved() {
        let mut generator = TerrainGenerator::new(1234);
        let chunks: [(i32, i32); 12] = [
            (0, 0),
            (1, 0),
            (0, 1),
            (-1, -1),
            (3, -2),
            (-240, -198),
            (96, 96),
            (-54, -96),
            (-204, 198),
            (10, -10),
            (40, 40),
            (-40, -40),
        ];
        let expected: [(u32, u32); 12] = [
            (2484963065, 47960),
            (3138134056, 48193),
            (491727624, 48021),
            (2503612913, 48243),
            (4278559287, 48074),
            (4215526530, 48593),
            (4036434587, 41565),
            (1560614313, 48808),
            (1586134614, 49451),
            (1322235985, 49678),
            (1605372176, 49419),
            (2286212515, 49408),
        ];

        for (i, &(chunk_x, chunk_z)) in chunks.iter().enumerate() {
            let primer = generator.generate(chunk_x, chunk_z);
            assert_eq!(
                primer.fingerprint(),
                expected[i].0,
                "chunk {chunk_x} {chunk_z} 指纹与 PHP 不一致"
            );
            let air = primer
                .ids()
                .iter()
                .flat_map(|column| column.iter())
                .filter(|&&id| id == 0)
                .count() as u32;
            assert_eq!(
                air, expected[i].1,
                "chunk {chunk_x} {chunk_z} 空气数与 PHP 不一致"
            );
        }
    }

    /// 打印单区块某段 y 的全部方块状态，用来和 PHP 逐格 diff 定位偏差。
    #[test]
    #[ignore]
    fn dump_columns() {
        let mut generator =
            TerrainGenerator::with_settings(1234, TerrainSettings::default().without_carvers());
        let primer = generator.generate(-120, 104);
        for x in 0..16_i32 {
            for z in 0..16_i32 {
                for y in 60..=100_i32 {
                    println!("{x} {z} {y} {}", primer.get_block_state(x, y, z));
                }
            }
        }
    }

    /// 统计原始高度分布，用来决定 128 高世界里怎么裁剪。
    #[test]
    #[ignore]
    fn height_stats() {
        let mut landscapes =
            LandscapeGenerator::new(RtgWorld::new(1234), BiomeProvider::new(1234), 16);
        let mut max = f64::MIN;
        let mut total = 0usize;
        let thresholds = [100.0, 120.0, 128.0, 144.0, 160.0, 192.0, 224.0];
        let mut over = [0usize; 7];
        for chunk_x in (-60..=60).step_by(2) {
            for chunk_z in (-60..=60).step_by(2) {
                let landscape = landscapes.get(chunk_x, chunk_z);
                for &height in landscape.noise.iter() {
                    max = max.max(height);
                    total += 1;
                    for (i, &t) in thresholds.iter().enumerate() {
                        if height > t {
                            over[i] += 1;
                        }
                    }
                }
            }
        }
        println!("max={max:.1} total={total}");
        for (i, &t) in thresholds.iter().enumerate() {
            println!("over {t}: {:.3}%", over[i] as f64 * 100.0 / total as f64);
        }
    }

    /// 扫一圈找每个 biome 第一次出现的区块，用来给黄金值补覆盖面。
    #[test]
    #[ignore]
    fn scan_biome_chunks() {
        let mut landscapes =
            LandscapeGenerator::new(RtgWorld::new(1234), BiomeProvider::new(1234), 16);
        let mut found: std::collections::BTreeMap<i32, (i32, i32)> = Default::default();
        // 粗扫 ±400（步长 4）＋几个热点细扫（步长 1），把小面积的 biome 变体也覆盖到
        let mut probe =
            |chunk_x: i32,
             chunk_z: i32,
             found: &mut std::collections::BTreeMap<i32, (i32, i32)>| {
                let landscape = landscapes.get(chunk_x, chunk_z);
                for &biome in landscape.biome.iter() {
                    found.entry(biome).or_insert((chunk_x, chunk_z));
                }
            };
        for chunk_x in (-400..=400).step_by(4) {
            for chunk_z in (-400..=400).step_by(4) {
                probe(chunk_x, chunk_z, &mut found);
            }
        }
        for (center_x, center_z) in [(108, 324), (352, 44), (-398, -36), (-400, -380), (60, 28)] {
            for dx in -40..=40 {
                for dz in -40..=40 {
                    probe(center_x + dx, center_z + dz, &mut found);
                }
            }
        }
        for (biome, (chunk_x, chunk_z)) in found {
            println!("BIOME {biome} {chunk_x} {chunk_z}");
        }
    }
}
