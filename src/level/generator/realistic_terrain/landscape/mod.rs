//! 对应 `rtg/ChunkLandscape.php` 与 `rtg/LandscapeGenerator.php`。
//!
//! 这是"高度图"的最后一层：对一个 16×16 区块，
//! 用 21×21 个 biome 采样点（间距 8 格）做距离加权混合，
//! 每个 biome 各自算一遍地形高度再按权重求和，最后交给 `BiomeAnalyzer` 修 biome。
//!
//! 权重表只算一次（静态），公式是 `1 - d^0.7 / 56^0.7`，截断到 0 以下不要。
//! 求和顺序会改变最后一位，所以 `weights` 里保留的是 `(采样下标, 权重)` 的插入序。

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::OnceLock;

use crate::level::generator::realistic_terrain::biome::BiomeProvider;
use crate::level::generator::realistic_terrain::biome::analyzer::BiomeAnalyzer;
use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::base;
use crate::level::generator::realistic_terrain::terrain::profiles;
use crate::level::generator::realistic_terrain::terrain::registry::create;
use crate::level::generator::realistic_terrain::terrain::water::WaterTerrain;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;

/// 一个区块的高度图（`x*16+z` 索引）、河流强度、biome 与 21×21 邻域。
#[derive(Clone)]
pub struct ChunkLandscape {
    pub noise: [f64; 256],
    pub river: [f64; 256],
    pub biome: [i32; 256],
    pub neighborhood: [i32; 441],
}

struct WeightTable {
    /// 每个格子对应一串 `(采样下标, 权重)`，只保留权重 > 0 的。
    weights: Vec<Vec<(u16, f64)>>,
    totals: Vec<f64>,
}

fn weight_table() -> &'static WeightTable {
    static TABLE: OnceLock<WeightTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let limit = f((56.0_f64 * 56.0).powf(0.7));
        let mut weights = Vec::with_capacity(256);
        let mut totals = Vec::with_capacity(256);

        for x in 0..16_i32 {
            for z in 0..16_i32 {
                let mut row: Vec<(u16, f64)> = Vec::new();
                for mx in 0..21_i32 {
                    for mz in 0..21_i32 {
                        let xd = (x - (mx - 8) * 8) as f64;
                        let zd = (z - (mz - 8) * 8) as f64;
                        let distance = f((xd * xd + zd * zd).powf(0.7));
                        let weight = f(1.0 - f(distance / limit));
                        if weight > 0.0 {
                            row.push(((mx * 21 + mz) as u16, weight));
                        }
                    }
                }
                let mut total = 0.0;
                for &(_, weight) in &row {
                    total = f(total + weight);
                }
                weights.push(row);
                totals.push(total);
            }
        }

        WeightTable { weights, totals }
    })
}

pub struct LandscapeGenerator {
    world: RtgWorld,
    provider: BiomeProvider,
    terrains: HashMap<i32, WaterTerrain>,
    cache: HashMap<(i32, i32), Arc<ChunkLandscape>>,
    order: VecDeque<(i32, i32)>,
    capacity: usize,
}

impl LandscapeGenerator {
    pub fn new(world: RtgWorld, provider: BiomeProvider, capacity: usize) -> Self {
        // PHP 是首次用到才建 `WaterTerrain`；这里一次性建好，
        // 因为地形对象无状态、构造是纯函数，两种做法结果一致，但省掉可变缓存。
        let mut terrains = HashMap::new();
        for id in 0..=255_i32 {
            if let Some(terrain) = create(id) {
                let profile = profiles::get(id).expect("profiles 与 registry 必须一一对应");
                terrains.insert(
                    id,
                    WaterTerrain::new(terrain, profile.allow_rivers, profile.allow_lakes),
                );
            }
        }

        Self {
            world,
            provider,
            terrains,
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn world(&self) -> &RtgWorld {
        &self.world
    }

    fn terrain(&self, id: i32) -> &WaterTerrain {
        self.terrains
            .get(&id)
            .unwrap_or_else(|| panic!("biome {id} 没有注册地形"))
    }

    /// `get($cx, $cz)`：算一个区块的高度图，带先进先出缓存。
    pub fn get(&mut self, chunk_x: i32, chunk_z: i32) -> Arc<ChunkLandscape> {
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.cache.get(&key) {
            return Arc::clone(cached);
        }

        let table = weight_table();
        let wx = chunk_x * 16;
        let wz = chunk_z * 16;

        let mut neighborhood = [0_i32; 441];
        let mut uniform = true;
        for x in -8..13 {
            for z in -8..13 {
                let id = self.provider.get_biome(wx + x * 8, wz + z * 8);
                let index = ((x + 8) * 21 + (z + 8)) as usize;
                neighborhood[index] = id;
                if id != neighborhood[0] {
                    uniform = false;
                }
            }
        }

        let mut landscape = ChunkLandscape {
            noise: [0.0; 256],
            river: [0.0; 256],
            biome: [0; 256],
            neighborhood,
        };

        for x in 0..16_i32 {
            for z in 0..16_i32 {
                let index = (x * 16 + z) as usize;
                let total = table.totals[index];

                // biome id -> 累计权重；BTreeMap 的键序就是 PHP `ksort(SORT_NUMERIC)` 的键序
                let mut weighted: BTreeMap<i32, f64> = BTreeMap::new();
                if uniform {
                    weighted.insert(neighborhood[0], total);
                } else {
                    for &(sample, weight) in &table.weights[index] {
                        let id = neighborhood[sample as usize];
                        let slot = weighted.entry(id).or_insert(0.0);
                        *slot = f(*slot + weight);
                    }
                }

                let mut mesa = 0.0;
                for (id, weight) in weighted.iter_mut() {
                    *weight = f(*weight / total);
                    if profiles::is_mesa(*id) {
                        mesa = f(mesa + *weight);
                    }
                }

                let world_x = wx + x;
                let world_z = wz + z;
                let river = base::get_river_strength(world_x, world_z, &self.world);
                landscape.river[index] = -river;

                let mut height = 0.0;
                let mut lake: Option<f64> = None;

                for (&id, &weight) in weighted.iter() {
                    let border = if profiles::is_mesa(id) { mesa } else { weight };
                    let terrain = self.terrain(id);

                    // 湖泊压力每个格子只按第一个"允许河流+湖泊"的 biome 算一次，
                    // 后续 biome 复用同一个值。
                    if lake.is_none() && terrain.allow_rivers && terrain.allow_lakes {
                        lake = Some(f(terrain.lake_pressure(
                            &self.world,
                            world_x as f64,
                            world_z as f64,
                            self.world.get_lake_frequency(),
                            self.world.get_lake_bend_size_large(),
                            self.world.get_lake_bend_size_medium(),
                            self.world.get_lake_bend_size_small(),
                        )));
                    }

                    let h = terrain.newr_noise(
                        &self.world,
                        world_x as f64,
                        world_z as f64,
                        border,
                        f(river + 1.0),
                        lake,
                    );
                    height = f(height + f(h * weight));
                }

                landscape.noise[index] = height;
                landscape.biome[index] = self.provider.get_biome(world_x, world_z);
            }
        }

        BiomeAnalyzer::repair(&mut landscape);

        if self.cache.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        let landscape = Arc::new(landscape);
        self.order.push_back(key);
        self.cache.insert(key, Arc::clone(&landscape));
        landscape
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 由 `php8.3 rtg_dump.php 1234 0 0 1 -2 -3 5` 从参考实现导出。
    /// 每块 256 行 `noise river biome`，高度是 float32 落地过的，可以位级比较。
    const GOLDEN: &str = include_str!("golden_landscape.txt");

    fn parse_golden() -> (i64, Vec<(i32, i32, Vec<(f64, f64, i32)>)>) {
        let mut seed = 0_i64;
        let mut chunks: Vec<(i32, i32, Vec<(f64, f64, i32)>)> = Vec::new();
        let mut current: Option<(i32, i32)> = None;
        let mut rows: Vec<(f64, f64, i32)> = Vec::new();

        for line in GOLDEN.lines().map(str::trim).filter(|l| !l.is_empty()) {
            if let Some(rest) = line.strip_prefix("seed ") {
                seed = rest.parse().expect("seed");
                continue;
            }
            if let Some(rest) = line.strip_prefix("chunk ") {
                if let Some((cx, cz)) = current.take() {
                    chunks.push((cx, cz, std::mem::take(&mut rows)));
                }
                let mut parts = rest.split_whitespace();
                current = Some((
                    parts.next().expect("cx").parse().expect("cx"),
                    parts.next().expect("cz").parse().expect("cz"),
                ));
                continue;
            }
            let mut parts = line.split_whitespace();
            let noise: f64 = parts.next().expect("noise").parse().expect("noise");
            let river: f64 = parts.next().expect("river").parse().expect("river");
            let biome: i32 = parts.next().expect("biome").parse().expect("biome");
            rows.push((noise, river, biome));
        }
        if let Some((cx, cz)) = current.take() {
            chunks.push((cx, cz, rows));
        }
        (seed, chunks)
    }

    #[test]
    fn matches_php_reference() {
        let (seed, chunks) = parse_golden();
        assert_eq!(chunks.len(), 3, "黄金值文件损坏");

        let world = RtgWorld::new(seed);
        let provider = BiomeProvider::new(seed);
        let mut generator = LandscapeGenerator::new(world, provider, 8);

        for (cx, cz, expected) in chunks {
            assert_eq!(expected.len(), 256, "区块 ({cx},{cz}) 黄金值行数不对");
            let actual = generator.get(cx, cz);

            let mut bad_noise = Vec::new();
            let mut bad_river = Vec::new();
            let mut bad_biome = Vec::new();

            for (i, &(noise, river, biome)) in expected.iter().enumerate() {
                // 两侧都已经是 float32 精确值，可以直接位级比较
                if (actual.noise[i] as f32) != (noise as f32) {
                    bad_noise.push((i, noise, actual.noise[i]));
                }
                if (actual.river[i] as f32) != (river as f32) {
                    bad_river.push((i, river, actual.river[i]));
                }
                if actual.biome[i] != biome {
                    bad_biome.push((i, biome, actual.biome[i]));
                }
            }

            assert!(
                bad_noise.is_empty() && bad_river.is_empty() && bad_biome.is_empty(),
                "区块 ({cx},{cz}) 与参考实现不一致：高度 {}/256 {:?}；河流 {}/256 {:?}；biome {}/256 {:?}",
                bad_noise.len(),
                &bad_noise[..bad_noise.len().min(3)],
                bad_river.len(),
                &bad_river[..bad_river.len().min(3)],
                bad_biome.len(),
                &bad_biome[..bad_biome.len().min(3)]
            );
        }
    }

    #[test]
    fn cache_returns_same_object() {
        let world = RtgWorld::new(1234);
        let provider = BiomeProvider::new(1234);
        let mut generator = LandscapeGenerator::new(world, provider, 2);

        let a = generator.get(0, 0);
        let again = generator.get(0, 0);
        assert!(Arc::ptr_eq(&a, &again), "同区块应命中同一个缓存对象");
    }

    #[test]
    fn heights_and_biomes_are_sane() {
        let world = RtgWorld::new(20260101);
        let provider = BiomeProvider::new(20260101);
        let mut generator = LandscapeGenerator::new(world, provider, 4);

        let mut seen = std::collections::HashSet::new();
        for (cx, cz) in [(0, 0), (1, 1), (-4, 7), (17, -3)] {
            let l = generator.get(cx, cz);
            for i in 0..256 {
                assert!(l.noise[i].is_finite(), "({cx},{cz})[{i}] 高度非有限");
                assert!(
                    (-64.0..=320.0).contains(&l.noise[i]),
                    "({cx},{cz})[{i}] 高度越界: {}",
                    l.noise[i]
                );
                assert!((0.0..=1.0).contains(&l.river[i]), "河流强度越界");
                assert!((0..=255).contains(&l.biome[i]), "biome id 越界");
                seen.insert(l.biome[i]);
            }
        }
        assert!(seen.len() > 2, "biome 太单一: {seen:?}");
    }
}
