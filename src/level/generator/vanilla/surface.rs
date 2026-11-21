//! `RandomLevelSource::buildSurfaces` + 各 biome 的 `buildSurfaceAt`，按 x86 libminecraftpe.so 逐条移植：
//!   Biome::buildSurfaceAtDefault @0x71c460   Biome::_placeBedrock @0x71c3d0
//!   ExtremeHillsBiome @0x71c900  TaigaBiome @0x71ca60  MutatedSavannaBiome @0x71c9c0
//!   SwampBiome @0x722470  MutatedBiome @0x6fd7a0（转交基础 biome）  MesaBiome（按 0.14.1 源码 MesaBiome.cpp）
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

/// 地表生成器：持有 SwampBiome 共用的 `PerlinSimplexNoise(Random(2345), 1)` 与恶地数据
#[derive(Clone)]
pub struct SurfaceBuilder {
    swamp_noise: PerlinSimplexNoise,
    mesa: Mesa,
}

// --- MesaBiome（0.14.1 源码 MesaBiome.cpp）---
const K_WHITE: u8 = 0;
const K_ORANGE: u8 = 1;
const K_YELLOW: u8 = 4;
const K_SILVER: u8 = 8;
const K_BROWN: u8 = 12;
const K_RED: u8 = 14;
const K_HARDENED: u8 = 16;
const STAINED_CLAY: u8 = 159;
const HARDENED_CLAY: u8 = 172;
const WATER: u8 = 8;

/// `MesaBiome::refreshBiome(seed)` 生成的数据。各恶地 biome 用同一个世界种子，色带完全相同。
#[derive(Clone)]
struct Mesa {
    bands: [u8; 64],
    band_noise: PerlinSimplexNoise,
    pillar_noise: PerlinSimplexNoise,
    roof_noise: PerlinSimplexNoise,
}

impl Mesa {
    fn new(seed: u32) -> Self {
        // generateBands
        let mut bands = [K_HARDENED; 64];
        let mut r = McpeRandom::new(seed);
        let band_noise = PerlinSimplexNoise::new(&mut r, 1);
        let mut i = 0i32;
        while i < 64 {
            i += r.next_int(5) as i32 + 1;
            if i < 64 {
                bands[i as usize] = K_ORANGE;
            }
            i += 1;
        }
        for (kind, len_rand, len_add) in [(K_YELLOW, 3, 1), (K_BROWN, 3, 2), (K_RED, 3, 1)] {
            let n = r.next_int(4) + 2;
            for _ in 0..n {
                let length = r.next_int(len_rand) as i32 + len_add;
                let start = r.next_int(64) as i32;
                let mut j = 0;
                while start + j < 64 && j < length {
                    bands[(start + j) as usize] = kind;
                    j += 1;
                }
            }
        }
        let whites = r.next_int(3) + 3;
        let mut pos = 0i32;
        for _ in 0..whites {
            pos += r.next_int(16) as i32 + 4;
            if pos < 64 {
                bands[pos as usize] = K_WHITE;
                if pos > 1 && r.next_boolean() {
                    bands[(pos - 1) as usize] = K_SILVER;
                }
                if pos < 63 && r.next_boolean() {
                    bands[(pos + 1) as usize] = K_SILVER;
                }
            }
        }
        // refreshBiome 再用同一种子建石柱噪声
        let mut r2 = McpeRandom::new(seed);
        let pillar_noise = PerlinSimplexNoise::new(&mut r2, 4);
        let roof_noise = PerlinSimplexNoise::new(&mut r2, 1);
        Self { bands, band_noise, pillar_noise, roof_noise }
    }

    fn band(&self, x: i32, y: i32, z: i32) -> u8 {
        let off = self.band_noise.get_value(x as f32 * 0.001953125, z as f32 * 0.001953125);
        let shift = ((off * 2.0) as f64).round_ties_even() as i32;
        self.bands[((y + shift + 64).rem_euclid(64)) as usize]
    }

    /// `MesaBiome::buildSurfaceAt`
    #[allow(clippy::too_many_arguments)]
    fn build(&self, s: &S, bryce: bool, forest: bool, rnd: &mut McpeRandom, chunk: &mut Chunk, x: usize, z: usize, wx: i32, wz: i32, noise: f32) {
        let n = (rnd.genrand() & 3) as usize + 2;
        for y in 0..n {
            chunk.set_block(x, y, z, BEDROCK);
        }
        let mut pillar_top = 0.0f32;
        if bryce {
            let (fx, fz) = (wx as f32, wz as f32);
            // 重建源码写作 ×0.1 与 35.5；x86 原版二进制（与 JE 1.7.10 相同）是 ×0.25 与 2.5
            let pillar = self.pillar_noise.get_value(0.25 * fx, 0.25 * fz).min(noise.abs());
            if pillar > 0.0 {
                let roof = self.roof_noise.get_value(fx * 0.001953125, fz * 0.001953125);
                let height = pillar * pillar * 2.5;
                let cap = (roof.abs() * 50.0).ceil() + 14.0;
                pillar_top = height.min(cap) + 64.0;
            }
        }
        let pillar_y = pillar_top as i32;
        let mut filler = s.filler;
        let mut run: i32 = -1;
        let mut layers = 0;
        let mut under_top = false;
        let depth = ((noise * 0.333_333_34 + 3.0) + rnd.next_float() * 0.25) as i32;
        // 重建源码写作 cosf(noise·π) > 0.5，但 x86 原版二进制（与 JE 1.7.10 相同）是
        // cosf(noise·(π·0.33333334)) > 0，这里以二进制为准
        let coarse = (noise * (std::f32::consts::PI * 0.333_333_34)).cos() > 0.0;
        let mut y: i32 = 127;
        loop {
            let yu = y as usize;
            let mut id = chunk.block(x, yu, z);
            if id == BEDROCK {
                break;
            }
            // 材质为 Air 或 Water 的方块在石柱高度以下填成石头
            if (id == AIR || id == WATER || id == STILL_WATER) && y < pillar_y {
                chunk.set_block(x, yu, z, STONE);
                id = STONE;
            }
            // 色带用完（layers > 14）后不再着色，但循环继续，以便把石柱下方的空腔填满
            if layers > 14 {
            } else if id == AIR {
                run = -1;
            } else if id == STONE {
                if run == -1 {
                    under_top = false;
                    if depth <= 0 {
                        filler = STONE;
                    } else if (59..=64).contains(&y) {
                        filler = s.filler;
                    }
                    run = depth + (y - 63).max(0);
                    if y < 62 {
                        chunk.set_block(x, yu, z, filler);
                        if filler == STAINED_CLAY {
                            chunk.set_meta(x, yu, z, K_ORANGE);
                        }
                    } else if forest && y > 86 + depth * 2 {
                        if coarse {
                            chunk.set_block(x, yu, z, DIRT);
                            chunk.set_meta(x, yu, z, 1);
                        } else {
                            chunk.set_block(x, yu, z, GRASS);
                        }
                    } else if y <= 66 + depth {
                        chunk.set_block(x, yu, z, s.top);
                        chunk.set_meta(x, yu, z, s.top_data);
                        under_top = true;
                    } else {
                        let band = if !(64..=127).contains(&y) {
                            K_ORANGE
                        } else if !coarse {
                            self.band(wx, y, wz)
                        } else {
                            K_HARDENED
                        };
                        if band < 16 {
                            chunk.set_block(x, yu, z, STAINED_CLAY);
                            chunk.set_meta(x, yu, z, band);
                        } else {
                            chunk.set_block(x, yu, z, HARDENED_CLAY);
                        }
                        layers += 1;
                    }
                } else if run > 0 {
                    run -= 1;
                    if under_top {
                        chunk.set_block(x, yu, z, STAINED_CLAY);
                        chunk.set_meta(x, yu, z, K_ORANGE);
                    } else {
                        let band = self.band(wx, y, wz);
                        if band < 16 {
                            chunk.set_block(x, yu, z, STAINED_CLAY);
                            chunk.set_meta(x, yu, z, band);
                        } else {
                            chunk.set_block(x, yu, z, HARDENED_CLAY);
                        }
                        layers += 1;
                    }
                }
            }
            y -= 1;
            if y == 0 {
                break;
            }
        }
    }
}


impl SurfaceBuilder {
    pub fn new(seed: u32) -> Self {
        let mut r = McpeRandom::new(2345);
        Self {
            mesa: Mesa::new(seed),
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
            K::Mesa => {
                // Bryce = 165（mesa 的变种）；森林 = 38、166
                let bryce = id == 165;
                let forest = id == 38 || id == 166;
                return self.mesa.build(&s, bryce, forest, rnd, chunk, x, z, wx, wz, noise);
            }
            K::Default => {}
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
