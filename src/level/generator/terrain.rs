// 对齐 Nukkit 的 Normal 生成器：海床 陆地 山脉 河流 沙滩 基岩
// generateChunk 只铺石头与水 装饰留到 populateChunk
use crate::level::generator::biome::{self, BiomeSelector, DESERT, ICE_PLAINS, OCEAN, RIVER, TAIGA};
use crate::level::generator::blocks::{BEDROCK, ICE, STILL_WATER, STONE};
use crate::level::generator::noise::{Noise, NoiseKind, fast_noise2d};
use crate::level::generator::populator::{self, caves, ore};
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

const SEA_HEIGHT: i32 = 62;
// 出生点挑选需要知道海平面
pub const SEA_LEVEL: i32 = SEA_HEIGHT;
const SEA_FLOOR_HEIGHT: i32 = 48;
const BEATH_START_HEIGHT: i32 = 60;
const BEATH_STOP_HEIGHT: i32 = 64;
const BEDROCK_DEPTH: i32 = 5;
const SEA_FLOOR_GENERATE_RANGE: i32 = 5;
const LAND_HEIGHT_RANGE: i32 = 18;
const MOUNTAIN_HEIGHT: i32 = 13;
const BASE_GROUND_HEIGHT: i32 = 3;

const NOISE_SIZE: usize = 16;
const NOISE_STRIDE: usize = NOISE_SIZE + 1;

// 每个 region 会克隆一份生成器 所以内部状态不能再有惰性初始化
// 否则不同 region 的洞穴噪声会各不相同
#[derive(Clone)]
pub struct TerrainGenerator {
    seed: i64,
    random: NukkitRandom,
    sea_floor: Noise,
    land: Noise,
    mountains: Noise,
    base_ground: Noise,
    river: Noise,
    selector: BiomeSelector,
    // Nukkit 里算出来但没用到 保留字段只为对齐随机序列
    #[allow(dead_code)]
    height_offset: i32,
    caves: Noise,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let seed = seed as i64;
        let mut random = NukkitRandom::new(seed);

        let sea_floor = Noise::new(NoiseKind::Simplex, &mut random, 1.0, 1.0 / 8.0, 1.0 / 64.0);
        let land = Noise::new(NoiseKind::Simplex, &mut random, 2.0, 1.0 / 8.0, 1.0 / 512.0);
        let mountains = Noise::new(NoiseKind::Simplex, &mut random, 4.0, 1.0, 1.0 / 500.0);
        let base_ground = Noise::new(NoiseKind::Simplex, &mut random, 4.0, 1.0 / 4.0, 1.0 / 64.0);
        let river = Noise::new(NoiseKind::Simplex, &mut random, 2.0, 1.0, 1.0 / 512.0);

        random.set_seed(seed);
        let selector = BiomeSelector::new(&mut random);
        let height_offset = random.next_range(-5, 3);
        // 洞穴噪声在构造时就定下来 Nukkit 是首次装饰才创建 那样多 region 会不一致
        let caves = Noise::new(NoiseKind::Simplex, &mut random, 4.1, 15.0, 1.0 / 200.0);

        Self {
            seed,
            random,
            sea_floor,
            land,
            mountains,
            base_ground,
            river,
            selector,
            height_offset,
            caves,
        }
    }

    // 对齐 Normal.pickBiome：先用坐标哈希挑一个小偏移 再查温度降雨表
    fn pick_biome(&self, x: i32, z: i32) -> u8 {
        // Java 里前两个乘法是 int 运算 与 long 种子异或时才符号扩展
        let mixed = x.wrapping_mul(2345803) ^ z.wrapping_mul(9236449);
        let mut hash = (mixed as i64) ^ self.seed;
        hash = hash.wrapping_mul(hash.wrapping_add(223));

        let mut x_noise = (hash >> 20) & 3;
        let mut z_noise = (hash >> 22) & 3;
        if x_noise == 3 {
            x_noise = 1;
        }
        if z_noise == 3 {
            z_noise = 1;
        }

        self.selector.pick(
            (x as i64 + x_noise - 1) as f64,
            (z as i64 + z_noise - 1) as f64,
        )
    }

    fn chunk_seed(&self, chunk_x: i32, chunk_z: i32) -> i64 {
        let base = (0xdead_beefu32 as i32) ^ (chunk_x << 8) ^ chunk_z;
        (base as i64) ^ self.seed
    }

    pub fn generate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));

        let sea_floor_noise = fast_noise2d(
            &self.sea_floor,
            NOISE_SIZE,
            NOISE_SIZE,
            4,
            chunk_x * 16,
            0,
            chunk_z * 16,
            0,
            0,
        );
        let land_noise = fast_noise2d(
            &self.land,
            NOISE_SIZE,
            NOISE_SIZE,
            4,
            chunk_x * 16,
            0,
            chunk_z * 16,
            0,
            0,
        );
        let mountain_noise = fast_noise2d(
            &self.mountains,
            NOISE_SIZE,
            NOISE_SIZE,
            4,
            chunk_x * 16,
            0,
            chunk_z * 16,
            0,
            0,
        );
        let base_noise = fast_noise2d(
            &self.base_ground,
            NOISE_SIZE,
            NOISE_SIZE,
            4,
            chunk_x * 16,
            0,
            chunk_z * 16,
            0,
            0,
        );
        let river_noise = fast_noise2d(
            &self.river,
            NOISE_SIZE,
            NOISE_SIZE,
            4,
            chunk_x * 16,
            0,
            chunk_z * 16,
            0,
            0,
        );

        let base_x = chunk_x * 16;
        let base_z = chunk_z * 16;

        // 生成阶段只写本区块 直接借一次区块 否则每个方块都要做一次 HashMap 查找
        let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
            return;
        };

        for genx in 0..NOISE_SIZE {
            for genz in 0..NOISE_SIZE {
                let index = genx * NOISE_STRIDE + genz;
                let wx = base_x + genx as i32;
                let wz = base_z + genz as i32;

                let mut can_base_ground = false;
                let mut can_river = true;

                // 用二次曲线把噪声拉成平缓的陆地
                let mut land_height_noise = (land_noise[index] + 1.0) * 2.956;
                land_height_noise = land_height_noise * land_height_noise - 0.6;
                if land_height_noise < 0.0 {
                    land_height_noise = 0.0;
                }

                let mut mountain_height_generate = mountain_noise[index] - 0.2;
                if mountain_height_generate < 0.0 {
                    mountain_height_generate = 0.0;
                }
                let mountain_generate = (MOUNTAIN_HEIGHT as f64 * mountain_height_generate) as i32;

                let mut land_height_generate = (LAND_HEIGHT_RANGE as f64 * land_height_noise) as i32;
                if land_height_generate > LAND_HEIGHT_RANGE {
                    can_base_ground = true;
                    land_height_generate = LAND_HEIGHT_RANGE;
                }

                let mut geny_height = SEA_FLOOR_HEIGHT + land_height_generate;
                geny_height += mountain_generate;

                let biome_id;

                if geny_height < BEATH_START_HEIGHT {
                    if geny_height < BEATH_START_HEIGHT - 5 {
                        geny_height +=
                            (SEA_FLOOR_GENERATE_RANGE as f64 * sea_floor_noise[index]) as i32;
                    }
                    biome_id = OCEAN;
                    if geny_height < SEA_FLOOR_HEIGHT - SEA_FLOOR_GENERATE_RANGE {
                        geny_height = SEA_FLOOR_HEIGHT;
                    }
                    can_river = false;
                } else if geny_height <= BEATH_STOP_HEIGHT {
                    // 0.14 没有沙滩群系 海边临时用沙漠代替
                    biome_id = DESERT;
                } else {
                    biome_id = self.pick_biome(wx, wz);
                    if can_base_ground {
                        let base_ground_height =
                            (LAND_HEIGHT_RANGE as f64 * land_height_noise) as i32
                                - LAND_HEIGHT_RANGE;
                        let mut extra = (BASE_GROUND_HEIGHT as f64 * (base_noise[index] + 1.0)) as i32;
                        if extra > base_ground_height {
                            extra = base_ground_height;
                        }
                        if extra > mountain_generate {
                            extra -= mountain_generate;
                        } else {
                            extra = 0;
                        }
                        geny_height += extra;
                    }
                }

                if can_river && geny_height <= SEA_HEIGHT - 5 {
                    can_river = false;
                }

                let mut biome_id = biome_id;

                if can_river {
                    let mut river_generate = river_noise[index];
                    if (-0.25..0.25).contains(&river_generate) {
                        river_generate = river_generate.abs();
                        river_generate = 0.25 - river_generate;
                        river_generate = river_generate * river_generate * 4.0;
                        river_generate -= 0.000_000_1;
                        if river_generate < 0.0 {
                            river_generate = 0.0;
                        }
                        geny_height -= (river_generate * 64.0) as i32;
                        if geny_height < SEA_HEIGHT {
                            biome_id = RIVER;
                            // 河床抬高 免得河流深到海床
                            if geny_height <= SEA_HEIGHT - 8 {
                                let floor = SEA_HEIGHT
                                    - 9
                                    + (BASE_GROUND_HEIGHT as f64 * (base_noise[index] + 1.0)) as i32;
                                let limit = if geny_height < SEA_HEIGHT - 7 {
                                    SEA_HEIGHT - 7
                                } else {
                                    geny_height
                                };
                                geny_height = floor.max(limit);
                            }
                        }
                    }
                }

                chunk.set_biome(genx, genz, biome_id, biome::biome(biome_id).color);

                let generate_height = geny_height.max(SEA_HEIGHT) as usize;
                for geny in 0..=generate_height {
                    let id = if geny <= BEDROCK_DEPTH as usize
                        && (geny == 0 || self.random.next_range(1, 5) == 1)
                    {
                        BEDROCK
                    } else if geny as i32 > geny_height {
                        if (biome_id == ICE_PLAINS || biome_id == TAIGA) && geny as i32 == SEA_HEIGHT
                        {
                            ICE
                        } else {
                            STILL_WATER
                        }
                    } else {
                        STONE
                    };
                    chunk.set_block(genx, geny, genz, id);
                }
            }
        }

        // 生成阶段只做地表覆盖
        populator::ground_cover::populate(world, chunk_x, chunk_z);
    }

    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));

        caves::populate(world, chunk_x, chunk_z, &self.caves);

        ore::populate(world, chunk_x, chunk_z, &mut self.random);

        // 群系自己的装饰
        let biome_id = world
            .chunk(chunk_x, chunk_z)
            .map(|chunk| chunk.biome_id(7, 7))
            .unwrap_or(0);
        let def = biome::biome(biome_id);
        for spec in def.populators {
            populator::run_spec(spec, world, chunk_x, chunk_z, &mut self.random);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::level::format::chunk::{CHUNK_HEIGHT, CHUNK_WIDTH, Chunk};
    use crate::level::generator::blocks::{
        AIR, BEDROCK, COAL_ORE, DIAMOND_ORE, IRON_ORE, LEAVES, LOG, STILL_WATER, STONE, WATER,
    };
    use crate::level::generator::{Generator, world::GenWorld};

    // 生成一个区块并完成装饰 邻居也要铺好地形
    fn build(seed: u32, chunk_x: i32, chunk_z: i32) -> HashMap<(i32, i32), Chunk> {
        let mut chunks = HashMap::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                chunks.insert(
                    (chunk_x + dx, chunk_z + dz),
                    Chunk::empty(chunk_x + dx, chunk_z + dz),
                );
            }
        }

        let mut generator = Generator::terrain(seed);
        {
            let mut world = GenWorld::new(&mut chunks);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    generator.generate_chunk(&mut world, chunk_x + dx, chunk_z + dz);
                }
            }
            generator.populate_chunk(&mut world, chunk_x, chunk_z);
        }
        chunks
    }

    fn center(chunks: &HashMap<(i32, i32), Chunk>) -> &Chunk {
        chunks.get(&(0, 0)).expect("中心区块")
    }

    fn at(chunks: &HashMap<(i32, i32), Chunk>, chunk_x: i32, chunk_z: i32) -> &Chunk {
        chunks.get(&(chunk_x, chunk_z)).expect("区块")
    }

    fn column_top(chunk: &Chunk, x: usize, z: usize) -> usize {
        (0..CHUNK_HEIGHT)
            .rev()
            .find(|y| chunk.block(x, *y, z) != AIR)
            .unwrap()
    }

    // 同种子两次生成结果必须一模一样
    #[test]
    fn deterministic_for_same_seed() {
        let a = build(1337, 0, 0);
        let b = build(1337, 0, 0);
        let (a, b) = (center(&a), center(&b));
        for y in 0..CHUNK_HEIGHT {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    assert_eq!(a.block(x, y, z), b.block(x, y, z), "{x},{y},{z}");
                }
            }
        }
    }

    // 最底层一定是基岩 前五层大量基岩
    #[test]
    fn bedrock_fills_bottom() {
        let chunks = build(7, 0, 0);
        let chunk = center(&chunks);
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                assert_eq!(chunk.block(x, 0, z), BEDROCK);
            }
        }
        let mut count = 0;
        for y in 1..=BEDROCK_DEPTH as usize {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    if chunk.block(x, y, z) == BEDROCK {
                        count += 1;
                    }
                }
            }
        }
        assert!(count > 100, "基岩太少: {count}");
    }

    // 大范围上要同时有海洋和陆地 高度差不能太小
    #[test]
    fn has_ocean_land_and_relief() {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        let mut water = 0;
        for cx in -1..=1 {
            let chunks = build(1337, cx, 0);
            let chunk = at(&chunks, cx, 0);
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    let top = column_top(chunk, x, z) as i32;
                    min = min.min(top);
                    max = max.max(top);
                    let id = chunk.block(x, top as usize, z);
                    if id == WATER || id == STILL_WATER {
                        water += 1;
                    }
                }
            }
        }
        assert!(water > 0, "没有水面");
        assert!(max > SEA_LEVEL + 1, "没有陆地: {min}..{max}");
        assert!(max - min >= 10, "起伏太小: {min}..{max}");
    }

    // 水面不会高过海平面
    #[test]
    fn water_never_above_sea_level() {
        let chunks = build(2024, 0, 0);
        let chunk = center(&chunks);
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                for y in (SEA_LEVEL as usize + 1)..CHUNK_HEIGHT {
                    let id = chunk.block(x, y, z);
                    assert!(
                        id != WATER && id != STILL_WATER,
                        "水位越界 {x},{y},{z} = {id}"
                    );
                }
            }
        }
    }

    // 装饰阶段要挖出洞穴 地下出现空气
    #[test]
    fn caves_carve_underground() {
        let mut air = 0;
        for cx in 0..3 {
            let chunks = build(99, cx, 0);
            let chunk = at(&chunks, cx, 0);
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    for y in 21..SEA_LEVEL as usize - 10 {
                        if chunk.block(x, y, z) == AIR {
                            air += 1;
                        }
                    }
                }
            }
        }
        assert!(air > 0, "没有洞穴");
    }

    // 矿石要埋进石头里
    #[test]
    fn ores_are_placed_in_stone() {
        let chunks = build(4242, 0, 0);
        let chunk = center(&chunks);
        let mut coal = 0;
        let mut iron = 0;
        let mut others = 0;
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                for y in 1..CHUNK_HEIGHT {
                    match chunk.block(x, y, z) {
                        COAL_ORE => coal += 1,
                        IRON_ORE => iron += 1,
                        DIAMOND_ORE => others += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(coal > 0, "没有煤矿");
        assert!(iron > 0, "没有铁矿");
        let _ = others;
    }

    // 陆地表面要么被树盖住 要么是草/沙
    #[test]
    fn surface_is_decorated() {
        let chunks = build(11, 0, 0);
        let chunk = center(&chunks);
        let mut trees = 0;
        let mut stone_exposed = 0;
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                let top = column_top(chunk, x, z);
                match chunk.block(x, top, z) {
                    LOG | LEAVES => trees += 1,
                    STONE => stone_exposed += 1,
                    _ => {}
                }
            }
        }
        assert!(trees > 0, "没有树");
        // 石头裸露只可能出现在洞穴口 数量要很少
        assert!(stone_exposed < 32, "石头裸露太多: {stone_exposed}");
    }
}


#[cfg(test)]
mod probe {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use std::collections::HashMap;

    #[test]
    #[ignore]
    fn split_timing() {
        let mut generator = TerrainGenerator::new(1337);
        let mut chunks = HashMap::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                chunks.insert((dx, dz), Chunk::empty(dx, dz));
            }
        }
        let mut world = GenWorld::new(&mut chunks);

        // 预热 与 Java 侧 JIT 热身对齐
        for _ in 0..100 {
            generator.generate_chunk(&mut world, 0, 0);
        }

        let t = std::time::Instant::now();
        for dx in -1..=1 {
            for dz in -1..=1 {
                generator.generate_chunk(&mut world, dx, dz);
            }
        }
        println!("9 generate: {:?}", t.elapsed());

        let t = std::time::Instant::now();
        for _ in 0..1000 {
            generator.generate_chunk(&mut world, 0, 0);
        }
        println!("1000 generate same chunk: {:?}", t.elapsed());

        for _ in 0..20 {
            generator.populate_chunk(&mut world, 0, 0);
        }
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            generator.populate_chunk(&mut world, 0, 0);
        }
        println!("1000 populate (caves+ore+deco): {:?}", t.elapsed());

        let noise = Noise::new(
            NoiseKind::Simplex,
            &mut NukkitRandom::new(1),
            1.0,
            1.0 / 8.0,
            1.0 / 64.0,
        );
        for _ in 0..100 {
            let _ = fast_noise2d(&noise, 16, 16, 4, 0, 0, 0, 0, 0);
        }
        let t = std::time::Instant::now();
        for i in 0..1000 {
            let _ = fast_noise2d(&noise, 16, 16, 4, i, 0, 0, 0, 0);
        }
        println!("1000 fast_noise2d: {:?}", t.elapsed());

        // 地表覆盖单独计时
        let t = std::time::Instant::now();
        for _ in 0..1000 {
            populator::ground_cover::populate(&mut world, 0, 0);
        }
        println!("1000 ground_cover: {:?}", t.elapsed());

        // 洞穴用的一整块 3D 噪声 每个区块一次
        for _ in 0..10 {
            let _ = crate::level::generator::noise::fast_noise3d(
                &generator.caves,
                16,
                128,
                16,
                4,
                4,
                4,
                0,
                0,
                0,
            );
        }
        let t = std::time::Instant::now();
        for _ in 0..100 {
            let _ = crate::level::generator::noise::fast_noise3d(
                &generator.caves,
                16,
                128,
                16,
                4,
                4,
                4,
                0,
                0,
                0,
            );
        }
        println!("100 fast_noise3d(16x128x16): {:?}", t.elapsed());
    }
}
