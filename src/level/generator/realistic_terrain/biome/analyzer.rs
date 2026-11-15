//! 对应 `rtg/biome/BiomeAnalyzer.php`（含 RTG 的 `SmoothingSearchStatus`）。
//!
//! 高度算完之后，biome 网格和高度网格并不自洽：河边可能标成陆地、
//! 海底可能标成内陆。这个类用 21×21 的邻域网格（441 格）做五轮修补：
//!
//! 1. 低洼 + 强河流 → 换成河流 biome
//! 2. 高于河岸却标成海洋/沙滩 → 换成最近的陆地
//! 3. 夹在水面和河岸之间 → 换成沙滩（自己的沙滩，或陆地邻居的沙滩）
//! 4. 低于水面却没标成水域 → 换成最近的海洋
//! 5. 仍在水下却还是陆地 → 换成河流 biome
//!
//! `hunt()` 的搜索顺序是 RTG 自己那套同心圆（`CircularSearchCreator`），
//! 同一个半径内的命中顺序直接影响结果，不能换成"最近邻"。

use std::sync::OnceLock;

use crate::level::generator::realistic_terrain::landscape::ChunkLandscape;
use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::profiles;

pub struct BiomeAnalyzer;

impl BiomeAnalyzer {
    /// 只关心 5 类：河流 / 海洋 / 沼泽 / 沙滩 / 其它陆地。
    pub fn flag(id: i32) -> i32 {
        match id {
            7 | 11 => 1,
            0 | 10 | 24 => 2,
            6 | 134 => 4,
            15 | 16 | 25 | 26 => 8,
            _ => 16,
        }
    }

    /// 把 `top` 朝水面（62）压下：河流越强压得越狠，先按河流占比归一一次。
    fn adjusted(top: f64, river: f64) -> f64 {
        if river >= 1.0 {
            return top;
        }
        let eroded = f(river / 0.09375);
        let mut top = top;
        if eroded <= 1.0 {
            top = f(f(top * f(1.0 - eroded)) + f(62.0 * eroded));
        }
        f(f(top * f(1.0 - river)) + f(62.0 * river))
    }

    /// 同心圆搜索顺序：从正右方开始顺时针绕一圈，再按半径从小到大收集。
    fn pattern() -> &'static Vec<usize> {
        static PATTERN: OnceLock<Vec<usize>> = OnceLock::new();
        PATTERN.get_or_init(|| {
            let mut order: Vec<(i32, i32)> = Vec::with_capacity(451);
            for y in 0..11 {
                for x in 10..21 {
                    order.push((x, y));
                }
            }
            for x in (10..=20).rev() {
                for y in 11..21 {
                    order.push((x, y));
                }
            }
            for y in (10..=20).rev() {
                for x in (0..=9).rev() {
                    order.push((x, y));
                }
            }
            for x in 0..10 {
                for y in (0..=10).rev() {
                    order.push((x, y));
                }
            }

            let distances: Vec<f64> = order
                .iter()
                .map(|&(x, y)| {
                    let dx = (x - 10) as f64;
                    let dy = (y - 10) as f64;
                    f((dx * dx + dy * dy).sqrt())
                })
                .collect();

            let mut found = [false; 441];
            let mut result: Vec<usize> = Vec::new();
            let mut radius = 0.0_f64;
            while radius < 9.5 {
                for (i, &(x, y)) in order.iter().enumerate() {
                    let index = (x * 21 + y) as usize;
                    if !found[index] && distances[i] <= radius {
                        found[index] = true;
                        result.push(index);
                    }
                }
                radius = f(radius + f(0.01));
            }
            result
        })
    }

    /// 在 3×3 邻域里各找一个符合 `flag` 的 biome，再按四象限双线性插值铺满 16×16。
    fn hunt(neighborhood: &[i32; 441], flag: i32) -> [i32; 256] {
        let pattern = Self::pattern();
        let n = pattern.len() as f64;

        let mut findings = [-1_i32; 9];
        let mut weights = [2.0_f64; 9];

        for xo in -1..=1 {
            for zo in -1..=1 {
                let location = ((xo + 1) * 3 + zo + 1) as usize;
                findings[location] = -1;
                weights[location] = 2.0;
                for (i, &index) in pattern.iter().enumerate() {
                    let biome = neighborhood[(index as i32 + xo * 21 + zo) as usize];
                    if Self::flag(biome) == flag {
                        findings[location] = biome;
                        weights[location] = f(f(f(n.sqrt()) - f((i as f64).sqrt())) + 2.0);
                        break;
                    }
                }
            }
        }

        let mut result = [-1_i32; 256];
        // (结果偏移, findings 起点)
        for &(bo, fo) in &[(0_usize, 0_usize), (128, 3), (8, 1), (136, 4)] {
            let corners = [
                findings[fo],
                findings[fo + 3],
                findings[fo + 1],
                findings[fo + 4],
            ];

            // `array_unique` 保序：先出现者优先，最终平票时也按这个顺序取。
            let mut biomes: Vec<i32> = Vec::with_capacity(4);
            for &corner in &corners {
                if !biomes.contains(&corner) {
                    biomes.push(corner);
                }
            }

            for x in 0..8_usize {
                for z in 0..8_usize {
                    if biomes.len() == 1 {
                        result[bo + x * 16 + z] = corners[0];
                        continue;
                    }

                    let mut w = [0.0_f64; 4];
                    for (ci, offset, cx, cz) in [
                        (0_usize, 0_usize, 7 - x, 7 - z),
                        (1, 3, x, 7 - z),
                        (2, 1, 7 - x, z),
                        (3, 4, x, z),
                    ] {
                        let id = corners[ci];
                        let v = f(f(weights[fo + offset] * cx as f64) * cz as f64);
                        for (slot, &biome) in biomes.iter().enumerate() {
                            if biome == id {
                                w[slot] = f(w[slot] + v);
                                break;
                            }
                        }
                    }

                    let mut best = 0.0;
                    let mut selected = -2;
                    for (slot, &biome) in biomes.iter().enumerate() {
                        if w[slot] > best {
                            best = w[slot];
                            selected = biome;
                        }
                    }
                    result[bo + x * 16 + z] = selected;
                }
            }
        }
        result
    }

    pub fn repair(landscape: &mut ChunkLandscape) {
        let noise = landscape.noise;
        let river = landscape.river;
        let neighborhood = landscape.neighborhood;
        let mut biomes = landscape.biome;

        let mut land: Option<[i32; 256]> = None;
        let mut beach: Option<[i32; 256]> = None;
        let mut ocean: Option<[i32; 256]> = None;

        // 1) 水下且河流很强：换成河流 biome
        for i in 0..256 {
            let id = biomes[i];
            let fl = Self::flag(id);
            if noise[i] <= 61.5 && river[i] > 0.7 && fl != 2 && fl != 4 {
                biomes[i] = profiles::get(id).map_or(id, |p| p.river_biome);
            }
        }

        // 2) 高出河岸却标成水域/沙滩：拉最近的陆地进来
        for i in 0..256 {
            if noise[i] < Self::adjusted(64.5, river[i]) {
                continue;
            }
            let fl = Self::flag(biomes[i]);
            if fl == 16 || fl == 4 {
                continue;
            }
            if land.is_none() {
                land = Some(Self::hunt(&neighborhood, 16));
            }
            let mut found = land.as_ref().expect("刚赋值")[i];
            if found == -1 {
                if beach.is_none() {
                    beach = Some(Self::hunt(&neighborhood, 8));
                }
                found = beach.as_ref().expect("刚赋值")[i];
            }
            if found != -1 {
                biomes[i] = found;
            }
        }

        // 3) 水面与河岸之间：铺沙滩
        for i in 0..256 {
            if noise[i] < 61.5
                || noise[i] > Self::adjusted(64.5, river[i])
                || Self::flag(biomes[i]) == 4
            {
                continue;
            }
            if beach.is_none() {
                beach = Some(Self::hunt(&neighborhood, 8));
                land = Some(Self::hunt(&neighborhood, 16));
            }
            let mut found = beach.as_ref().expect("刚赋值")[i];
            if found != -1 {
                let land_found = land.as_ref().expect("刚赋值")[i];
                if land_found > -1 {
                    found = profiles::get(land_found).map_or(found, |p| p.beach_biome);
                }
                biomes[i] = found;
            }
        }

        // 4) 水面以下却没标成水域：换成最近的海洋
        for i in 0..256 {
            if noise[i] > 61.5 {
                continue;
            }
            let fl = Self::flag(biomes[i]);
            if fl == 2 || fl == 4 || fl == 1 {
                continue;
            }
            if ocean.is_none() {
                ocean = Some(Self::hunt(&neighborhood, 2));
            }
            let found = ocean.as_ref().expect("刚赋值")[i];
            if found != -1 {
                biomes[i] = found;
            }
        }

        // 5) 还在水下但仍是陆地：换成河流 biome
        for i in 0..256 {
            if noise[i] <= 61.5 && Self::flag(biomes[i]) == 16 {
                biomes[i] = profiles::get(biomes[i]).map_or(biomes[i], |p| p.river_biome);
            }
        }

        landscape.biome = biomes;
    }
}
