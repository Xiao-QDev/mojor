// 地形生成器：忠实还原 MCPE 0.14.3 地形 = JE 1.6.4 ChunkProviderGenerate
// 高度场用 5×17×5 网格（世界高 128），密度公式见 example/mcpe_reader/je164.py
use crate::level::generator::biome;
use crate::level::generator::genlayer::BiomeGen;
use crate::level::generator::blocks::{AIR, BEDROCK, STILL_WATER, STONE};
use crate::level::generator::mcpe_noise::NoiseStack;
use crate::level::generator::noise::{Noise, NoiseKind};
use crate::level::generator::populator::{self, caves, ore};
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

pub const SEA_LEVEL: i32 = 62;
pub const BEDROCK_DEPTH: i32 = 5;

// JE 1.6.4 的 scale 三元组（aet.java:217-223）
const S_MAIN: (f64, f64, f64) = (684.412 / 80.0, 684.412 / 160.0, 684.412 / 80.0);
const S_LIMIT: (f64, f64, f64) = (684.412, 684.412, 684.412);
// MCPE 的 C++ 版 2D 噪声实测按 y=0 / yScale=0.5 处理（Java 的 y=10 语义不适用，
// 试过 y=10 + yScale=1.0 会把 RMSE 从 2.47 拉到 4.37）
const S_DEPTH: (f64, f64, f64) = (200.0, 200.0, 0.5);

/// biome 的 depth / scale（acq.java:7-29，另加 1.7 的 24/25）
fn biome_de(id: u8) -> (f64, f64) {
    match id {
        0 => (-1.0, 0.4),
        1 => (0.1, 0.3),
        2 => (0.1, 0.2),
        3 => (0.3, 1.5),
        4 => (0.1, 0.3),
        5 => (0.1, 0.4),
        6 => (-0.2, 0.1),
        7 => (-0.5, 0.0),
        10 => (-1.0, 0.5),
        11 => (-0.5, 0.0),
        12 => (0.1, 0.3),
        13 => (0.3, 1.3),
        14 => (0.2, 1.0),
        15 => (-1.0, 0.1),
        16 => (0.0, 0.1),
        17 => (0.3, 0.8),
        18 => (0.3, 0.7),
        19 => (0.3, 0.8),
        20 => (0.2, 0.3),
        21 => (0.2, 0.4),
        22 => (1.8, 0.5),
        24 => (-1.8, 0.1),
        25 => (0.1, 0.8),
        _ => (0.1, 0.3),
    }
}

pub struct TerrainGenerator {
    seed: u32,
    stack: NoiseStack,
    biomes: BiomeGen,
    caves: Noise,
    random: NukkitRandom,
}

// biome 链内部有可变随机状态，clone 时重建一份而不是共享
impl Clone for TerrainGenerator {
    fn clone(&self) -> Self {
        Self {
            seed: self.seed,
            stack: self.stack.clone(),
            biomes: BiomeGen::new(self.seed as i64),
            caves: self.caves.clone(),
            random: self.random.clone(),
        }
    }
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let stack = NoiseStack::new(seed);
        let mut random = NukkitRandom::new(seed as i64);
        // biome 改由 GenLayer 链生成（对齐 MCPE 0.14.3），不再用 Nukkit 噪声选择器
        let biomes = BiomeGen::new(seed as i64);
        let caves = Noise::new(NoiseKind::Simplex, &mut random, 4.1, 15.0, 1.0 / 200.0);
        Self {
            seed,
            stack,
            biomes,
            caves,
            random,
        }
    }

    fn pick_biome(&self, x: i32, z: i32) -> u8 {
        self.biomes.biome_at(x, z)
    }

    fn chunk_seed(&self, chunk_x: i32, chunk_z: i32) -> i64 {
        let base = (0xdead_beefu32 as i32) ^ (chunk_x << 8) ^ chunk_z;
        (base as i64) ^ self.seed as i64
    }

    /// 5×5 网格点的 biome 加权 depth/scale（aet.java:229-254）
    fn biome_grid(&self, bio: &[[u8; 10]; 10]) -> ([[f64; 5]; 5], [[f64; 5]; 5]) {
        let mut kern = [[0.0f64; 5]; 5];
        for dx in -2i32..=2 {
            for dz in -2i32..=2 {
                kern[(dx + 2) as usize][(dz + 2) as usize] =
                    10.0 / (((dx * dx + dz * dz) as f64) + 0.2).sqrt();
            }
        }
        let mut dm = [[0.1f64; 5]; 5];
        let mut em = [[0.3f64; 5]; 5];
        for i in 0..5usize {
            for j in 0..5usize {
                let (cd, _) = biome_de(bio[i + 2][j + 2]);
                let (mut se, mut sd, mut sw) = (0.0f64, 0.0f64, 0.0f64);
                for dx in -2i32..=2 {
                    for dz in -2i32..=2 {
                        let (d, e) = biome_de(
                            bio[(i as i32 + dx + 2) as usize][(j as i32 + dz + 2) as usize],
                        );
                        let mut w = kern[(dx + 2) as usize][(dz + 2) as usize] / (d + 2.0);
                        if d > cd {
                            w /= 2.0;
                        }
                        se += e * w;
                        sd += d * w;
                        sw += w;
                    }
                }
                em[i][j] = se / sw * 0.9 + 0.1;
                dm[i][j] = (sd / sw * 4.0 - 1.0) / 8.0;
            }
        }
        (dm, em)
    }

    pub fn generate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        // 4 格分辨率 biome（MCPE `fillRawBiomeData` = 1:4 主层 getInts(cx*4-2, cz*4-2, 10, 10)）
        let bio = self.biomes.raw_chunk(chunk_x, chunk_z);
        self.build_chunk(world, chunk_x, chunk_z, &bio);
    }

    /// 用给定 4 格分辨率 biome 生成区块（biome 由外部提供时用，便于定点验证）
    pub(crate) fn build_chunk(
        &mut self,
        world: &mut GenWorld,
        chunk_x: i32,
        chunk_z: i32,
        bio: &[[u8; 10]; 10],
    ) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));
        let base_x = chunk_x * 16;
        let base_z = chunk_z * 16;
        let (dm, em) = self.biome_grid(bio);

        // 5×17×5 高度场
        let pos = (chunk_x as f64 * 4.0, 0.0, chunk_z as f64 * 4.0);
        let min_l = self.stack.min_limit.get_region(pos, 5, 17, 5, S_LIMIT);
        let max_l = self.stack.max_limit.get_region(pos, 5, 17, 5, S_LIMIT);
        let main_n = self.stack.main_noise.get_region(pos, 5, 17, 5, S_MAIN);
        let depth_n = self.stack.depth_noise.get_region(pos, 5, 1, 5, S_DEPTH);

        // 密度（aet.java:255-308）
        let mut dens = [[0.0f64; 17]; 25];
        for i in 0..5usize {
            for j in 0..5usize {
                let idx = i * 5 + j;
                let d50 = {
                    let v = depth_n[idx] / 8000.0;
                    let v = (if v < 0.0 { -v * 0.3 } else { v }) * 3.0 - 2.0;
                    if v < 0.0 {
                        (v / 2.0).max(-1.0) / 1.4 / 2.0
                    } else {
                        v.min(1.0) / 8.0
                    }
                };
                let v56 = (dm[i][j] + d50 * 0.2) * 17.0 / 16.0;
                let anchor = 8.5 + v56 * 4.0;
                let scale = em[i][j];
                for k in 0..17usize {
                    let mut v32 = (k as f64 - anchor) * 12.0 / scale;
                    if v32 < 0.0 {
                        v32 *= 4.0;
                    }
                    let v34 = min_l[idx * 17 + k] / 512.0;
                    let v36 = max_l[idx * 17 + k] / 512.0;
                    let v38 = (main_n[idx * 17 + k] / 10.0 + 1.0) / 2.0;
                    let base = if v38 < 0.0 {
                        v34
                    } else if v38 > 1.0 {
                        v36
                    } else {
                        v34 + (v36 - v34) * v38
                    };
                    let mut d = base - v32;
                    if k > 13 {
                        let f = (k as f64 - 13.0) / 3.0;
                        d = d * (1.0 - f) - 10.0 * f;
                    }
                    dens[idx][k] = d;
                }
            }
        }

        // surface 噪声（aet.java:111）：4 八度、scale 0.03125*2
        let surf = self.stack.surface.get_region(
            (base_x as f64, base_z as f64, 0.0),
            16,
            16,
            1,
            (0.0625, 0.0625, 0.0625),
        );

        // 基岩深度逐列预生成，避免与区块可变借用冲突
        let mut bed = [[0usize; 16]; 16];
        for x in 0..16 {
            for z in 0..16 {
                bed[x][z] = self.random.next_range(0, 5) as usize;
            }
        }

        let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
            return;
        };

        // biome 存每 block 的精确值（GenLayer 链是逐 block 的），4 格数组只用于密度插值
        for z in 0..16usize {
            for x in 0..16usize {
                let b = self.pick_biome(base_x + x as i32, base_z + z as i32);
                let color = biome::biome(b).color;
                chunk.set_biome(x, z, b, color);
            }
        }

        // 三线性插值 + 填充（aet.java:55-105）
        for i in 0..4usize {
            for j in 0..4usize {
                for k in 0..16usize {
                    let g = |a: usize, b: usize, c: usize| dens[a * 5 + b][c];
                    let c000 = g(i, j, k);
                    let c100 = g(i + 1, j, k);
                    let c010 = g(i, j + 1, k);
                    let c110 = g(i + 1, j + 1, k);
                    let c001 = g(i, j, k + 1);
                    let c101 = g(i + 1, j, k + 1);
                    let c011 = g(i, j + 1, k + 1);
                    let c111 = g(i + 1, j + 1, k + 1);
                    for dy in 0..8usize {
                        let fy = dy as f64 / 8.0;
                        let y = k * 8 + dy;
                        for dx in 0..4usize {
                            let fx = dx as f64 / 4.0;
                            let x = i * 4 + dx;
                            for dz in 0..4usize {
                                let fz = dz as f64 / 4.0;
                                let z = j * 4 + dz;
                                let l00 = c000 + (c100 - c000) * fx;
                                let l10 = c010 + (c110 - c010) * fx;
                                let l01 = c001 + (c101 - c001) * fx;
                                let l11 = c011 + (c111 - c011) * fx;
                                let l0 = l00 + (l10 - l00) * fz;
                                let l1 = l01 + (l11 - l01) * fz;
                                let d = l0 + (l1 - l0) * fy;
                                let id = if y <= bed[x][z] {
                                    BEDROCK
                                } else if d > 0.0 {
                                    STONE
                                } else if (y as i32) <= SEA_LEVEL {
                                    STILL_WATER
                                } else {
                                    AIR
                                };
                                chunk.set_block(x, y, z, id);
                            }
                        }
                    }
                }
            }
        }

        // surface 层（aet.java:108-167）：从 y=127 向下，首个石头处铺草/沙，
        // 厚度 = s/3 + 3 + rand*0.25，以下依次铺 filler
        let mut thick = [[0i32; 16]; 16];
        for z in 0..16 {
            for x in 0..16 {
                thick[x][z] =
                    (surf[x * 16 + z] / 3.0 + 3.0 + self.random.next_double() * 0.25) as i32;
            }
        }
        let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
            return;
        };
        for z in 0..16 {
            for x in 0..16 {
                let def = biome::biome(self.pick_biome(base_x + x as i32, base_z + z as i32));
                let cover = def.ground_cover;
                if cover.is_empty() {
                    continue;
                }
                let top = cover[0];
                let filler = cover[1.min(cover.len() - 1)];
                let mut left = -1i32;
                for y in (0..128usize).rev() {
                    if y <= bed[x][z] {
                        break;
                    }
                    let cur = chunk.block(x, y, z);
                    if cur == AIR {
                        left = -1;
                    } else if cur == STONE {
                        if left == -1 {
                            left = thick[x][z];
                            let id = if y >= 62 { top } else { filler };
                            chunk.set_block(x, y, z, id);
                        } else if left > 0 {
                            left -= 1;
                            chunk.set_block(x, y, z, filler);
                        }
                    }
                }
            }
        }
    }

    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));

        caves::populate(world, chunk_x, chunk_z, &self.caves);
        ore::populate(world, chunk_x, chunk_z, &mut self.random);

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

/// 与存档比对脚本的 CLEAN 掩码完全一致：只算地形实心方块，
/// 排除水/冰/植被/树，否则「水面高度 62」会把统计抬起来（原口径不可比）。
#[cfg(test)]
fn is_clean_ground(id: u8) -> bool {
    matches!(
        id,
        1 | 2 | 3 | 12 | 13 | 24 | 80 | 82 | 96 | 110 | 159 | 161 | 172 | 243
    )
}

#[cfg(test)]
fn ground_top(chunk: &crate::level::format::chunk::Chunk, x: usize, z: usize) -> i32 {
    (0..crate::level::format::chunk::CHUNK_HEIGHT)
        .rev()
        .find(|y| is_clean_ground(chunk.block(x, *y, z)))
        .map(|y| y as i32)
        .unwrap_or(0)
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

    // 最底层一定是基岩
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
    // 注：地形密度算法已与真实存档对齐（RMSE 2.47），但 biome 仍用 Nukkit 的
    // pick_biome 近似，不产生海洋。等 GenLayer 实现后恢复。
    #[test]
    fn has_ocean_land_and_relief() {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        let mut water = 0;
        for cx in -4..=4 {
            let chunks = build(1152860609, cx, 0);
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
                        "y={y} 有水 id={id}"
                    );
                }
            }
        }
    }

    // 煤铁钻石矿应该存在（钻石稀少，扫多个区块）
    #[test]
    fn ores_are_generated() {
        let chunks = build(42, 0, 0);
        let chunk = center(&chunks);
        let mut coal = 0;
        let mut iron = 0;
        for y in 0..CHUNK_HEIGHT {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    match chunk.block(x, y, z) {
                        COAL_ORE => coal += 1,
                        IRON_ORE => iron += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(coal > 0, "没有煤矿");
        assert!(iron > 0, "没有铁矿");

        // 钻石生成概率低，扫多个区块
        let mut diamond = 0;
        'diamond: for cx in -4..=4i32 {
            for cz in -4..=4i32 {
                let chunks = build(42, cx, cz);
                let chunk = at(&chunks, cx, cz);
                for y in 0..16usize {
                    for x in 0..CHUNK_WIDTH {
                        for z in 0..CHUNK_WIDTH {
                            if chunk.block(x, y, z) == DIAMOND_ORE {
                                diamond += 1;
                                break 'diamond;
                            }
                        }
                    }
                }
            }
        }
        assert!(diamond > 0, "没有钻石矿");
    }

    // 陆地表面要么被树盖住 要么是草/沙 扫描多个种子和区块确保覆盖
    #[test]
    fn surface_is_decorated() {
        let mut trees = 0;
        'outer: for seed in [11u32, 100, 999, 1337, 42] {
            for cx in -3..=3i32 {
                let chunks = build(seed, cx, 0);
                let chunk = at(&chunks, cx, 0);
                for x in 0..CHUNK_WIDTH {
                    for z in 0..CHUNK_WIDTH {
                        let top = column_top(chunk, x, z);
                        if matches!(chunk.block(x, top, z), LOG | LEAVES) {
                            trees += 1;
                            break 'outer;
                        }
                    }
                }
            }
        }
        assert!(trees > 0, "没有树");

        // 石头裸露检查：陆地区块不应该满是裸石
        let chunks = build(1337, 0, 0);
        let chunk = center(&chunks);
        let mut stone_exposed = 0;
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                let top = column_top(chunk, x, z);
                if chunk.block(x, top, z) == STONE {
                    stone_exposed += 1;
                }
            }
        }
        assert!(stone_exposed < 32, "石头裸露太多: {stone_exposed}");
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use std::collections::HashMap;
    use crate::level::format::chunk::Chunk;
    use crate::level::generator::{Generator, world::GenWorld};

    fn build(seed: u32, cx: i32, cz: i32) -> HashMap<(i32, i32), Chunk> {
        let mut chunks = HashMap::new();
        for dx in -1..=1 { for dz in -1..=1 {
            chunks.insert((cx+dx, cz+dz), Chunk::empty(cx+dx, cz+dz));
        }}
        let mut generator = Generator::terrain(seed);
        {
            let mut world = GenWorld::new(&mut chunks);
            for dx in -1..=1 { for dz in -1..=1 {
                generator.generate_chunk(&mut world, cx+dx, cz+dz);
            }}
            generator.populate_chunk(&mut world, cx, cz);
        }
        chunks
    }

    #[test]
    #[ignore]
    fn biome_debug() {
        for cx in -2..=2i32 {
            let chunks = build(11, cx, 0);
            let chunk = chunks.get(&(cx, 0)).unwrap();
            let biome = chunk.biome_id(7, 7);
            let top = (0..128usize).rev()
                .find(|y| chunk.block(7, *y, 7) != 0)
                .unwrap_or(0);
            let top_block = chunk.block(7, top, 7);
            println!("cx={cx} biome={biome} top={top} block={top_block}");
        }
    }

    #[test]
    #[ignore]
    fn split_timing() {
        let start = std::time::Instant::now();
        for cx in -8..=8 { for cz in -8..=8 {
            build(42, cx, cz);
        }}
        println!("17x17 chunks: {:?}", start.elapsed());
    }

    // 导出用于与真实存档逐列对比的高度（test1: seed 1152860609, chunk 63,0）
    #[test]
    #[ignore]
    fn dump_heights_test1() {
        let chunks = build(1_152_860_609, 63, 0);
        let c = chunks.get(&(63, 0)).unwrap();
        for z in 0..16 {
            let row: Vec<i32> = (0..16)
                .map(|x| {
                    (0..128)
                        .rev()
                        .find(|y| c.block(x, *y, z) != 0)
                        .map(|y| y as i32)
                        .unwrap_or(0)
                })
                .collect();
            println!("HEIGHT {row:?}");
        }
    }

    // chunk 63,0 的真实 4 格分辨率 biome（由 exp_bio.py 从存档导出）
    const REAL_BIO: [[u8; 10]; 10] = [
        [7, 7, 7, 16, 16, 0, 0, 0, 0, 0],
        [7, 7, 7, 7, 16, 0, 0, 16, 16, 0],
        [7, 7, 19, 16, 16, 16, 16, 16, 16, 16],
        [19, 19, 19, 19, 19, 16, 16, 16, 16, 16],
        [19, 19, 19, 19, 19, 19, 16, 16, 16, 16],
        [19, 19, 19, 19, 19, 19, 19, 19, 5, 16],
        [19, 19, 19, 19, 19, 19, 19, 19, 16, 5],
        [19, 19, 19, 19, 19, 19, 19, 19, 19, 5],
        [19, 19, 19, 19, 19, 19, 19, 19, 19, 19],
        [19, 19, 19, 19, 19, 19, 19, 19, 19, 19],
    ];

    // 批量导出多个 chunk 高度，供外部脚本统计（test1: seed 1152860609）
    #[test]
    #[ignore]
    fn dump_heights_multi() {
        let list = [
            (63, 0), (63, 1), (64, 0), (65, -1), (60, 0),
            (68, -2), (70, -5), (75, -6), (52, 9), (56, 12),
        ];
        for (cx, cz) in list {
            let chunks = build(1_152_860_609, cx, cz);
            let c = match chunks.get(&(cx, cz)) {
                Some(c) => c,
                None => continue,
            };
            for z in 0..16 {
                let row: Vec<i32> = (0..16).map(|x| ground_top(c, x, z)).collect();
                println!("H {cx} {cz} {row:?}");
            }
        }
    }

    #[test]
    #[ignore]
    fn dump_heights_realbio() {
        let mut chunks = HashMap::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                chunks.insert((63 + dx, dz), Chunk::empty(63 + dx, dz));
            }
        }
        let mut g = match Generator::terrain(1_152_860_609) {
            Generator::Terrain(t) => t,
            _ => panic!("terrain"),
        };
        {
            let mut world = GenWorld::new(&mut chunks);
            g.build_chunk(&mut world, 63, 0, &REAL_BIO);
        }
        let c = chunks.get(&(63, 0)).unwrap();
        for z in 0..16 {
            let row: Vec<i32> = (0..16)
                .map(|x| {
                    (0..128)
                        .rev()
                        .find(|y| c.block(x, *y, z) != 0)
                        .map(|y| y as i32)
                        .unwrap_or(0)
                })
                .collect();
            println!("HEIGHT {row:?}");
        }
    }
}
