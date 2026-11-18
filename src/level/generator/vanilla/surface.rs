//! `RandomLevelSource::buildSurfaces` + 各 biome 的 `buildSurfaceAt`，按 x86 libminecraftpe.so 逐条移植：
//!   Biome::buildSurfaceAtDefault @0x71c460   Biome::_placeBedrock @0x71c3d0
//!   ExtremeHillsBiome @0x71c900  TaigaBiome @0x71ca60  MutatedSavannaBiome @0x71c9c0
//!   SwampBiome @0x722470  MutatedBiome @0x6fd7a0（转交基础 biome）  MesaBiome @0x721ce0（未移植，暂走默认）
//! biome 参数表（top/filler/温度/类型）由模拟运行原版 Biome::initBiomes 后直接导出。

use crate::level::format::chunk::Chunk;
use crate::level::generator::blocks::{AIR, BEDROCK, DIRT, GRASS, GRAVEL, ICE, SAND, SANDSTONE, STILL_WATER, STONE, WATER_LILY};
use crate::level::generator::mcpe_noise::{McpeRandom, PerlinSimplexNoise};

const PODZOL: u8 = 243;

#[derive(Clone, Copy, PartialEq, Eq)]
enum K {
    Default,
    ExtremeHills,
    Taiga,
    Swamp,
    MutatedSavanna,
    Mesa,
    Mutated,
}

#[derive(Clone, Copy)]
struct S {
    kind: K,
    /// ExtremeHills/Taiga：`Biome+0x9c` 的类型；Mutated：基础 biome id
    arg: u32,
    top: u8,
    top_data: u8,
    filler: u8,
    temp: f32,
}

fn surf(id: u8) -> S {
    match id {
        0 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f000000) },
        1 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f4ccccd) },
        2 => S { kind: K::Default, arg: 0, top: 12, top_data: 0, filler: 12, temp: f32::from_bits(0x40000000) },
        3 => S { kind: K::ExtremeHills, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e4ccccd) },
        4 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f333333) },
        5 => S { kind: K::Taiga, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e800000) },
        6 => S { kind: K::Swamp, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f4ccccd) },
        7 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f000000) },
        8 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x40000000) },
        9 => S { kind: K::Default, arg: 0, top: 3, top_data: 0, filler: 3, temp: f32::from_bits(0x3f000000) },
        10 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x00000000) },
        11 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x00000000) },
        12 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x00000000) },
        13 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x00000000) },
        14 => S { kind: K::Default, arg: 0, top: 110, top_data: 0, filler: 3, temp: f32::from_bits(0x3f666666) },
        15 => S { kind: K::Default, arg: 0, top: 110, top_data: 0, filler: 3, temp: f32::from_bits(0x3f666666) },
        16 => S { kind: K::Default, arg: 0, top: 12, top_data: 0, filler: 12, temp: f32::from_bits(0x3f4ccccd) },
        17 => S { kind: K::Default, arg: 0, top: 12, top_data: 0, filler: 12, temp: f32::from_bits(0x40000000) },
        18 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f333333) },
        19 => S { kind: K::Taiga, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e800000) },
        20 => S { kind: K::ExtremeHills, arg: 1, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e4ccccd) },
        21 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f733333) },
        22 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f733333) },
        23 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f733333) },
        24 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f000000) },
        25 => S { kind: K::Default, arg: 0, top: 1, top_data: 0, filler: 1, temp: f32::from_bits(0x3e4ccccd) },
        26 => S { kind: K::Default, arg: 0, top: 12, top_data: 0, filler: 12, temp: f32::from_bits(0x3d4ccccd) },
        27 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f19999a) },
        28 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f19999a) },
        29 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f333333) },
        30 => S { kind: K::Taiga, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0xbf000000) },
        31 => S { kind: K::Taiga, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0xbf000000) },
        32 => S { kind: K::Taiga, arg: 1, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e99999a) },
        33 => S { kind: K::Taiga, arg: 1, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e99999a) },
        34 => S { kind: K::ExtremeHills, arg: 1, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e4ccccd) },
        35 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f99999a) },
        36 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f800000) },
        37 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        38 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        39 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        129 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f4ccccd) },
        130 => S { kind: K::Mutated, arg: 2, top: 12, top_data: 0, filler: 12, temp: f32::from_bits(0x40000000) },
        131 => S { kind: K::ExtremeHills, arg: 2, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e4ccccd) },
        132 => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f333333) },
        133 => S { kind: K::Mutated, arg: 5, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e800000) },
        134 => S { kind: K::Mutated, arg: 6, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f4ccccd) },
        140 => S { kind: K::Default, arg: 0, top: 80, top_data: 0, filler: 3, temp: f32::from_bits(0x00000000) },
        149 => S { kind: K::Mutated, arg: 21, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f733333) },
        151 => S { kind: K::Mutated, arg: 23, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f733333) },
        155 => S { kind: K::Mutated, arg: 27, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f19999a) },
        156 => S { kind: K::Mutated, arg: 28, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f19999a) },
        157 => S { kind: K::Mutated, arg: 29, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f333333) },
        158 => S { kind: K::Mutated, arg: 30, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0xbf000000) },
        160 => S { kind: K::Taiga, arg: 2, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e800000) },
        161 => S { kind: K::Mutated, arg: 33, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e99999a) },
        162 => S { kind: K::ExtremeHills, arg: 2, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3e4ccccd) },
        163 => S { kind: K::MutatedSavanna, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f8ccccd) },
        164 => S { kind: K::MutatedSavanna, arg: 0, top: 2, top_data: 0, filler: 3, temp: f32::from_bits(0x3f800000) },
        165 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        166 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        167 => S { kind: K::Mesa, arg: 0, top: 12, top_data: 1, filler: 172, temp: f32::from_bits(0x40000000) },
        _ => S { kind: K::Default, arg: 0, top: 2, top_data: 0, filler: 3, temp: 0.5 },
    }
}

/// 地表生成器：持有 SwampBiome 共用的 `PerlinSimplexNoise(Random(2345), 1)`
#[derive(Clone)]
pub struct SurfaceBuilder {
    swamp_noise: PerlinSimplexNoise,
}

impl Default for SurfaceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceBuilder {
    pub fn new() -> Self {
        let mut r = McpeRandom::new(2345);
        Self {
            swamp_noise: PerlinSimplexNoise::new(&mut r, 1),
        }
    }

    /// `loadChunk` 里的区块随机数：`MT19937(cx*0x14609048 + cz*0x7ebe2d5)`
    pub fn chunk_random(chunk_x: i32, chunk_z: i32) -> McpeRandom {
        let s = (chunk_x as u32)
            .wrapping_mul(0x1460_9048)
            .wrapping_add((chunk_z as u32).wrapping_mul(0x07eb_e2d5));
        McpeRandom::new(s)
    }

    /// `buildSurfaces`：先 x 后 z 逐列调用 biome 的 `buildSurfaceAt`。
    /// `biomes` 为每方块 biome（`[x + z*16]`），`noise` 为地表噪声（`[z*16 + x]`）。
    pub fn build(&self, chunk: &mut Chunk, chunk_x: i32, chunk_z: i32, biomes: &[u8; 256], noise: &[f32], rnd: &mut McpeRandom) {
        for x in 0..16usize {
            for z in 0..16usize {
                let b = biomes[x + z * 16];
                let wx = chunk_x * 16 + x as i32;
                let wz = chunk_z * 16 + z as i32;
                self.build_at(b, rnd, chunk, x, z, wx, wz, noise[z * 16 + x]);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_at(&self, id: u8, rnd: &mut McpeRandom, chunk: &mut Chunk, x: usize, z: usize, wx: i32, wz: i32, noise: f32) {
        let mut s = surf(id);
        match s.kind {
            K::Mutated => return self.build_at(s.arg as u8, rnd, chunk, x, z, wx, wz, noise),
            K::ExtremeHills => {
                s.top = GRASS;
                s.top_data = 0;
                s.filler = DIRT;
                if (noise > 0.2 || noise < -0.1) && s.arg == 2 {
                    s.top = GRAVEL;
                    s.filler = GRAVEL;
                } else if noise > 1.0 && s.arg != 1 {
                    s.top = STONE;
                    s.filler = STONE;
                }
            }
            K::Taiga => {
                if s.arg == 1 || s.arg == 2 {
                    s.top = GRASS;
                    s.top_data = 0;
                    s.filler = DIRT;
                    if noise > 1.75 {
                        s.top = DIRT;
                        s.top_data = 1;
                    } else if noise as f64 > -0.95 {
                        s.top = PODZOL;
                    }
                }
            }
            K::MutatedSavanna => {
                s.top = GRASS;
                s.top_data = 0;
                s.filler = DIRT;
                if noise > 1.75 {
                    s.top = STONE;
                    s.filler = STONE;
                } else if noise > -0.5 {
                    s.top = DIRT;
                    s.top_data = 1;
                }
            }
            K::Swamp => {
                let v = self.swamp_noise.get_value(wx as f32 * 0.25, wz as f32 * 0.25);
                if v > 0.0 {
                    // 原版怪癖：循环找的是自上而下**第一个材质为 Air（MaterialType 0）**的方块
                    // （Material::isType(0)），几乎总是 y=127，所以这段基本不会生效；照搬。
                    // 若该处正好 y=62 且不是静水 → 换静水，v<0.12 再放睡莲
                    if let Some(y) = (0..128usize).rev().find(|&y| chunk.block(x, y, z) == AIR) {
                        if y == 62 && chunk.block(x, y, z) != STILL_WATER {
                            chunk.set_block(x, 62, z, STILL_WATER);
                            if v < 0.12 {
                                chunk.set_block(x, 63, z, WATER_LILY);
                            }
                        }
                    }
                }
            }
            K::Mesa | K::Default => {}
        }
        default_surface(&s, rnd, chunk, x, z, noise);
    }
}

/// `Biome::buildSurfaceAtDefault`
fn default_surface(s: &S, rnd: &mut McpeRandom, chunk: &mut Chunk, x: usize, z: usize, noise: f32) {
    let depth = ((noise * 0.333_333_34 + 3.0) + rnd.next_float() * 0.25) as i32;
    // _placeBedrock：y < (genrand&3)+2 全部基岩
    let n = (rnd.genrand() & 3) as usize + 2;
    for y in 0..n {
        chunk.set_block(x, y, z, BEDROCK);
    }
    let (mut top, mut td, mut filler) = (s.top, s.top_data, s.filler);
    let mut run: i32 = -1;
    let mut y: usize = 127;
    loop {
        let blk = chunk.block(x, y, z);
        if blk == BEDROCK {
            return;
        }
        if blk == AIR {
            run = -1;
        } else if blk == STONE {
            let yi = y as i32;
            if run == -1 {
                if depth <= 0 {
                    top = AIR;
                    td = 0;
                    filler = blk;
                } else if (59..=64).contains(&yi) {
                    top = s.top;
                    td = s.top_data;
                    filler = s.filler;
                }
                if yi <= 62 && top == AIR {
                    top = if s.temp < 0.15 { ICE } else { STILL_WATER };
                    td = 0;
                }
                run = depth;
                if yi >= 62 {
                    chunk.set_block(x, y, z, top);
                    chunk.set_meta(x, y, z, td);
                } else if yi < 56 - depth {
                    top = AIR;
                    filler = STONE;
                    chunk.set_block(x, y, z, GRAVEL);
                } else {
                    chunk.set_block(x, y, z, filler);
                }
            } else if run > 0 {
                run -= 1;
                chunk.set_block(x, y, z, filler);
                if run == 0 && filler == SAND {
                    run = (rnd.genrand() & 3) as i32 + (yi - 63).max(0);
                    filler = SANDSTONE;
                }
            }
        }
        y -= 1;
        if y == 0 {
            return;
        }
    }
}
