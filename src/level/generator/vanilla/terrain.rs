// 地形生成器：忠实还原 MCPE 0.14.3 地形 = JE 1.6.4 ChunkProviderGenerate
// 高度场用 5×17×5 网格（世界高 128），密度公式见 example/mcpe_reader/je164.py
use crate::level::generator::biome;
use crate::level::generator::genlayer::BiomeGen;
use crate::level::generator::blocks::{AIR, STILL_WATER, STONE};
use crate::level::generator::mcpe_noise::NoiseStack;
use crate::level::generator::populator::{self, ore};
use crate::level::generator::vanilla::surface::SurfaceBuilder;
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

pub const SEA_LEVEL: i32 = 62;
pub const BEDROCK_DEPTH: i32 = 5;

// JE 1.6.4 的 scale 三元组（aet.java:217-223）
// getHeights 里的比例尺立即数（0x4108e1e5 / 0x4088e1e5 / 0x442b1a5e / 0x4455e0f6）
const S_MAIN: (f32, f32, f32) = (8.55515, 4.277575, 8.55515);
const S_LO: (f32, f32, f32) = (684.412, 855.515, 684.412); // +0x20
const S_HI: (f32, f32, f32) = (684.412, 684.412, 684.412); // +0x24
// MCPE 的 C++ 版 2D 噪声实测按 y=0 / yScale=0.5 处理（Java 的 y=10 语义不适用，
// 试过 y=10 + yScale=1.0 会把 RMSE 从 2.47 拉到 4.37）
const S_DEPTH: (f32, f32) = (200.0, 200.0); // +0x34，2D

/// biome 的 (depth, scale) = `Biome+0x64 / +0x68`。
/// 直接在 unicorn 里运行原版 `Biome::initBiomes`（含 `.init_array` 里的 `BiomeHeight` 常量）后
/// 逐个读出的 f32 位模式，与 MCPE 0.14 逐位相同（与 JE 1.7.10 有 5/20/38/133/161 等几处不同）。
fn biome_de(id: u8) -> (f32, f32) {
    match id {
        0 => (f32::from_bits(0xbf800000), f32::from_bits(0x3dcccccd)),
        1 => (f32::from_bits(0x3e000000), f32::from_bits(0x3d4ccccd)),
        2 => (f32::from_bits(0x3e000000), f32::from_bits(0x3d4ccccd)),
        3 => (f32::from_bits(0x3f800000), f32::from_bits(0x3f000000)),
        4 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        5 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        6 => (f32::from_bits(0xbe4ccccd), f32::from_bits(0x3dcccccd)),
        7 => (f32::from_bits(0xbf000000), f32::from_bits(0x00000000)),
        8 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        9 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        10 => (f32::from_bits(0xbf800000), f32::from_bits(0x3dcccccd)),
        11 => (f32::from_bits(0xbf000000), f32::from_bits(0x00000000)),
        12 => (f32::from_bits(0x3e000000), f32::from_bits(0x3d4ccccd)),
        13 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        14 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3e99999a)),
        15 => (f32::from_bits(0x00000000), f32::from_bits(0x3ccccccd)),
        16 => (f32::from_bits(0x00000000), f32::from_bits(0x3ccccccd)),
        17 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        18 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        19 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        20 => (f32::from_bits(0x3f4ccccd), f32::from_bits(0x3ecccccd)),
        21 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        22 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        23 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        24 => (f32::from_bits(0xbfe66666), f32::from_bits(0x3dcccccd)),
        25 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3f4ccccd)),
        26 => (f32::from_bits(0x00000000), f32::from_bits(0x3ccccccd)),
        27 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        28 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        29 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        30 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3e4ccccd)),
        31 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        32 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3e4ccccd)),
        33 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        34 => (f32::from_bits(0x3f800000), f32::from_bits(0x3f000000)),
        35 => (f32::from_bits(0x3e000000), f32::from_bits(0x3d4ccccd)),
        36 => (f32::from_bits(0x3fc00000), f32::from_bits(0x3ccccccd)),
        37 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        38 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        39 => (f32::from_bits(0x3fc00000), f32::from_bits(0x3ccccccd)),
        129 => (f32::from_bits(0x3e000000), f32::from_bits(0x3d4ccccd)),
        130 => (f32::from_bits(0x3e666666), f32::from_bits(0x3e800000)),
        131 => (f32::from_bits(0x3f800000), f32::from_bits(0x3f000000)),
        132 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3ecccccd)),
        133 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3ecccccd)),
        134 => (f32::from_bits(0xbdcccccd), f32::from_bits(0x3e99999a)),
        140 => (f32::from_bits(0x3ed9999a), f32::from_bits(0x3ee66667)),
        149 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3ecccccd)),
        151 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3ecccccd)),
        155 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3ecccccd)),
        156 => (f32::from_bits(0x3f0ccccd), f32::from_bits(0x3f000000)),
        157 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3ecccccd)),
        158 => (f32::from_bits(0x3e99999a), f32::from_bits(0x3ecccccd)),
        160 => (f32::from_bits(0x3e4ccccd), f32::from_bits(0x3e4ccccd)),
        161 => (f32::from_bits(0x3f0ccccd), f32::from_bits(0x3f000000)),
        162 => (f32::from_bits(0x3f800000), f32::from_bits(0x3f000000)),
        163 => (f32::from_bits(0x3eb9999a), f32::from_bits(0x3f9ccccd)),
        164 => (f32::from_bits(0x3f866666), f32::from_bits(0x3f9b3334)),
        165 => (f32::from_bits(0x3dcccccd), f32::from_bits(0x3e4ccccd)),
        166 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),
        167 => (f32::from_bits(0x3ee66666), f32::from_bits(0x3e99999a)),

        _ => (f32::from_bits(0xbf800000), f32::from_bits(0x3dcccccd)), // 未注册 id → ocean
    }
}

/// `RandomLevelSource` ctor 写入 this+0x3c 的 5×5 核（10/sqrt(dx²+dz²+0.2) 的 f32 立即数）
const KERNEL: [[f32; 5]; 5] = {
    const A: f32 = f32::from_bits(0x405f7f69);
    const B: f32 = f32::from_bits(0x408c544c);
    const C: f32 = f32::from_bits(0x409c24de);
    const D: f32 = f32::from_bits(0x40d7be73);
    const E: f32 = f32::from_bits(0x41120f31);
    const F: f32 = f32::from_bits(0x41b2e2ac);
    [[A, B, C, B, A], [B, D, E, D, B], [C, E, F, E, C], [B, D, E, D, B], [A, B, C, B, A]]
};

pub struct TerrainGenerator {
    seed: u32,
    stack: NoiseStack,
    biomes: BiomeGen,
    surface: SurfaceBuilder,
    random: NukkitRandom,
}

// biome 链内部有可变随机状态，clone 时重建一份而不是共享
impl Clone for TerrainGenerator {
    fn clone(&self) -> Self {
        Self {
            seed: self.seed,
            stack: self.stack.clone(),
            biomes: BiomeGen::new(self.seed as i64),
            surface: self.surface.clone(),
            random: self.random.clone(),
        }
    }
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let stack = NoiseStack::new(seed);
        let random = NukkitRandom::new(seed as i64);
        // biome 改由 GenLayer 链生成（对齐 MCPE 0.14.3），不再用 Nukkit 噪声选择器
        let biomes = BiomeGen::new(seed as i64);
        Self {
            seed,
            stack,
            biomes,
            surface: SurfaceBuilder::new(),
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

    pub fn generate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        // 4 格分辨率 biome（MCPE `fillRawBiomeData` = 1:4 主层 getInts(cx*4-2, cz*4-2, 10, 10)）
        let bio = self.biomes.raw_chunk(chunk_x, chunk_z);
        self.build_chunk(world, chunk_x, chunk_z, &bio);
    }

    /// 调试/对拍用：返回 (地表前方块, 地表后方块, 地表后 meta, 每方块 biome, 地表噪声, 洞穴后方块)，
    /// 方块按原版布局 `x<<11 | z<<7 | y`
    pub fn surface_stages(&mut self, chunk_x: i32, chunk_z: i32) -> (Vec<u8>, Vec<u8>, Vec<u8>, [u8; 256], Vec<f32>, Vec<u8>) {
        use crate::level::format::chunk::Chunk;
        let dump = |c: &Chunk| -> (Vec<u8>, Vec<u8>) {
            let mut b = vec![0u8; 32768];
            let mut m = vec![0u8; 32768];
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..128 {
                        b[x << 11 | z << 7 | y] = c.block(x, y, z);
                        m[x << 11 | z << 7 | y] = c.meta(x, y, z);
                    }
                }
            }
            (b, m)
        };
        let mut chunks = std::collections::HashMap::new();
        chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
        let bio = self.biomes.raw_chunk(chunk_x, chunk_z);
        {
            let mut world = GenWorld::new(&mut chunks);
            self.build_chunk_inner(&mut world, chunk_x, chunk_z, &bio, 0);
        }
        let (pre, _) = dump(&chunks[&(chunk_x, chunk_z)]);
        let mut chunks2 = std::collections::HashMap::new();
        chunks2.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
        {
            let mut world = GenWorld::new(&mut chunks2);
            self.build_chunk_inner(&mut world, chunk_x, chunk_z, &bio, 1);
        }
        let (post, meta) = dump(&chunks2[&(chunk_x, chunk_z)]);
        let mut chunks3 = std::collections::HashMap::new();
        chunks3.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
        {
            let mut world = GenWorld::new(&mut chunks3);
            self.build_chunk_inner(&mut world, chunk_x, chunk_z, &bio, 2);
        }
        let (cave, _) = dump(&chunks3[&(chunk_x, chunk_z)]);
        let biomes = *self.biomes.chunk_biomes(chunk_x, chunk_z);
        (pre, post, meta, biomes, self.surface_noise(chunk_x, chunk_z), cave)
    }

    /// `buildSurfaces` 的地表噪声：PerlinSimplexNoise(+0x2c)，`[z*16 + x]`
    pub fn surface_noise(&self, chunk_x: i32, chunk_z: i32) -> Vec<f32> {
        self.stack.surface.get_region(
            ((chunk_x * 16) as f32, (chunk_z * 16) as f32),
            16,
            16,
            (0.125, 0.125),
            1.0,
            0.5,
        )
    }

    /// 5×5×17 密度网格 = `RandomLevelSource::getHeights`（@0x7cc6a0）逐条移植，`[x*5+z][k]`。
    /// 全程 f32、运算顺序与原版一致；biome 核是 3×3（取 5×5 抛物线核的中心 3×3）。
    pub fn density(&self, chunk_x: i32, chunk_z: i32, bio: &[[u8; 10]; 10]) -> [[f32; 17]; 25] {
        let (px, pz) = ((chunk_x * 4) as f32, (chunk_z * 4) as f32);
        let depth_n = self.stack.depth_noise.get_region_2d((px, pz), 5, 5, S_DEPTH);
        let pos = (px, 0.0, pz);
        let main_n = self.stack.main_noise.get_region(pos, 5, 17, 5, S_MAIN);
        let lo_n = self.stack.min_limit.get_region(pos, 5, 17, 5, S_LO);
        let hi_n = self.stack.max_limit.get_region(pos, 5, 17, 5, S_HI);

        let mut dens = [[0.0f32; 17]; 25];
        for i in 0..5usize {
            for j in 0..5usize {
                let idx = i * 5 + j;
                let cd = biome_de(bio[i + 2][j + 2]).0;
                let (mut sw, mut ss, mut sd) = (0.0f32, 0.0f32, 0.0f32);
                for a in 0..3usize {
                    // 一次处理一列的 3 个 dz（-1, 0, +1），累加顺序照原版
                    let mut w = [0.0f32; 3];
                    let mut d = [0.0f32; 3];
                    let mut sc = [0.0f32; 3];
                    for b in 0..3usize {
                        let (dd, s) = biome_de(bio[i + 1 + a][j + 1 + b]);
                        let raw = KERNEL[1 + b][1 + a] / (2.0 + dd);
                        w[b] = if cd < dd { 0.5 * raw } else { raw };
                        d[b] = dd;
                        sc[b] = s;
                    }
                    sw = (sw + w[2]) + (w[0] + w[1]);
                    ss = (sc[2] * w[2] + ss) + (sc[0] * w[0] + sc[1] * w[1]);
                    sd = (d[1] * w[1] + d[0] * w[0]) + (w[2] * d[2] + sd);
                }
                let avg_s = ss / sw;
                let avg_d = sd / sw;
                let scale = avg_s * 0.9 + 0.1;
                let dt = (avg_d * 4.0 + -1.0) * 0.125;
                let mut dn = 0.000125f32 * depth_n[idx];
                if dn < 0.0 {
                    dn = -dn * 0.3;
                }
                dn = dn * 3.0 - 2.0;
                dn = if dn < 0.0 {
                    (dn * 0.5).max(-1.0) * 0.357_142_87
                } else {
                    dn.min(1.0) * 0.125
                };
                let off = dn * 0.2 + dt;
                let base = -8.5f32 - off * 4.25;
                let ys = 12.0f32 / scale;
                for k in 0..17usize {
                    let mut v = (k as f32 + base) * ys;
                    if v < 0.0 {
                        v *= 4.0;
                    }
                    let t = (0.1f32 * main_n[idx * 17 + k] + 1.0) * 0.5;
                    let lo = 0.003_906_25f32 * lo_n[idx * 17 + k];
                    let mut val = lo;
                    if t >= 0.0 {
                        let hi = 0.001_953_125f32 * hi_n[idx * 17 + k];
                        val = if t > 1.0 { hi } else { (hi - lo) * t + lo };
                    }
                    let mut dv = val - v;
                    if k > 13 {
                        let q = (k - 13) as f32 * 0.333_333_34;
                        dv = dv * (1.0 - q) + q * -10.0;
                    }
                    dens[idx][k] = dv;
                }
            }
        }
        dens
    }

    /// 用给定 4 格分辨率 biome 生成区块（biome 由外部提供时用，便于定点验证）
    pub(crate) fn build_chunk(
        &mut self,
        world: &mut GenWorld,
        chunk_x: i32,
        chunk_z: i32,
        bio: &[[u8; 10]; 10],
    ) {
        self.build_chunk_inner(world, chunk_x, chunk_z, bio, 2);
    }

    fn build_chunk_inner(
        &mut self,
        world: &mut GenWorld,
        chunk_x: i32,
        chunk_z: i32,
        bio: &[[u8; 10]; 10],
        stage: u8,
    ) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));
        let base_x = chunk_x * 16;
        let base_z = chunk_z * 16;
        let dens = self.density(chunk_x, chunk_z, bio);

        // surface 噪声（aet.java:111）：4 八度、scale 0.03125*2
        let surf = self.surface_noise(chunk_x, chunk_z);

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

        // 三线性插值 + 填充 = `RandomLevelSource::prepareHeights`（@0x814220）。
        // 原版用增量累加（f32）：y 每步 +0.125·Δ，x 每步 +0.25·Δ，z 方向首格直接取角值。
        // 密度 > 0 → 石头；否则 y ≤ 62 → 静水，其余空气。基岩在 buildSurfaces 里铺。
        for cx in 0..4usize {
            for cz in 0..4usize {
                for ck in 0..16usize {
                    let g = |a: usize, b: usize, c: usize| dens[a * 5 + b][c];
                    let mut a0 = g(cx, cz, ck);
                    let mut b0 = g(cx, cz + 1, ck);
                    let mut c0 = g(cx + 1, cz, ck);
                    let mut d0 = g(cx + 1, cz + 1, ck);
                    let sa = 0.125f32 * (g(cx, cz, ck + 1) - a0);
                    let sb = 0.125f32 * (g(cx, cz + 1, ck + 1) - b0);
                    let sc = 0.125f32 * (g(cx + 1, cz, ck + 1) - c0);
                    let sd = 0.125f32 * (g(cx + 1, cz + 1, ck + 1) - d0);
                    for yy in 0..8usize {
                        let y = ck * 8 + yy;
                        let xs1 = (c0 - a0) * 0.25;
                        let xs2 = (d0 - b0) * 0.25;
                        let mut a = a0;
                        let mut b = b0;
                        for xx in 0..4usize {
                            let step = (b - a) * 0.25;
                            let mut v = a;
                            for zz in 0..4usize {
                                if zz > 0 {
                                    v += step;
                                }
                                let id = if v > 0.0 {
                                    STONE
                                } else if y as i32 <= SEA_LEVEL {
                                    STILL_WATER
                                } else {
                                    AIR
                                };
                                chunk.set_block(cx * 4 + xx, y, cz * 4 + zz, id);
                            }
                            a += xs1;
                            b += xs2;
                        }
                        a0 += sa;
                        b0 += sb;
                        c0 += sc;
                        d0 += sd;
                    }
                }
            }
        }

        if stage == 0 {
            return;
        }
        // buildSurfaces：MCPE 区块随机数 + 各 biome 的 buildSurfaceAt（含基岩）
        let biomes = self.biomes.chunk_biomes(chunk_x, chunk_z);
        let mut rnd = SurfaceBuilder::chunk_random(chunk_x, chunk_z);
        let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
            return;
        };
        self.surface.build(chunk, chunk_x, chunk_z, &biomes, &surf, &mut rnd);
        // loadChunk：无限世界（RandomLevelSource ctor 的 bool = 0）在地表之后挖洞穴
        if stage >= 2 {
            crate::level::generator::vanilla::caves::apply(chunk, chunk_x, chunk_z, self.seed);
        }
    }

    pub fn populate_chunk(&mut self, world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
        self.random.set_seed(self.chunk_seed(chunk_x, chunk_z));

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
