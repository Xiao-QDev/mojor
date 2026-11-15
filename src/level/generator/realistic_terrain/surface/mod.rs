//! 对应 `rtg/surface/`：把地形噪声柱涂成真正的方块。
//!
//! 每个 biome 一套规则（PHP 里是 59 个 `SurfaceVanillaXxx` 类）。PHP 这层代码是
//! 从 Java 机械转出来的，56 个类里只有 26 种不同的循环体，所以这里用
//! `Kind` 区分算法族、字段存每 biome 的参数，代码结构与 PHP 一一对应。
//!
//! 坐标约定与 PHP 一致：`i/j` 是世界坐标，`x/z` 是区块内 0..15。
//!
//! `paintTerrain` 在 PHP 里就是 10 个参数，这里照搬，故整模块放开该 lint。
#![allow(clippy::too_many_arguments)]

use crate::level::generator::realistic_terrain::chunk_primer::ChunkPrimer;
use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::base::calc_cliff;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;

/// `SurfaceBase` 的默认方块（RTG 的 `state = id << 4 | meta` 记法）。
const SHADOW_STONE: i32 = 2553;
const SHADOW_DESERT: i32 = 2551;
const CLIFF_STONE: i32 = 16;
const CLIFF_COBBLE: i32 = 64;

pub struct Surface {
    pub biome_id: i32,
    top_block: i32,
    filler_block: i32,
    shadow_stone_block: i32,
    #[allow(dead_code)]
    shadow_desert_block: i32,
    cliff_stone_block: i32,
    cliff_cobble_block: i32,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Kind {
    /// `$c > 1.4` 的简化版：Plains/SunflowerPlains/JungleEdge(M)/JungleM/RoofedForestM/
    /// BirchForestHillsM/ColdTaigaM。
    Simple { cliff: f64 },
    /// 带 `noise3f` 可变悬崖 + 单层 mix 噪声：BirchForest( Hills/M )/Jungle/FlowerForest/
    /// ExtremeHillsPlus(M)/RoofedForest。
    Mixed {
        min: f64,
        s_cliff: f64,
        s_height: f64,
        s_strength: f64,
        c_cliff: f64,
        mix_block: i32,
        mix_height: f64,
    },
    /// `Mixed` 加第二层 mix 噪声：Forest/ForestHills。
    Mixed2 {
        min: f64,
        s_cliff: f64,
        s_height: f64,
        s_strength: f64,
        c_cliff: f64,
        mix_block: i32,
        mix_height: f64,
        mix2_block: i32,
        mix2_height: f64,
    },
    /// 雪地针叶林：ColdTaiga/MegaTaiga/MegaSpruceTaiga/Taiga(Hills/M)，常量写死。
    SnowyMix,
    /// 雪地丘陵：ColdTaigaHills/MegaTaigaHills/RedwoodTaigaHills。
    SnowyCliff {
        min: f64,
        s_cliff: f64,
        s_height: f64,
        s_strength: f64,
        i_cliff: f64,
        i_height: f64,
        i_strength: f64,
        c_cliff: f64,
    },
    /// 悬崖状态机、无 mix：MushroomIsland(Shore)/JungleHills/DesertHills/DesertM。
    CliffState {
        min: f64,
        s_cliff: f64,
        s_height: f64,
        s_strength: f64,
        c_cliff: f64,
        top1: CliffBlock,
        top2: i32,
        below1: i32,
        below2: i32,
        k63: bool,
    },
    /// 双层噪声 mix 的顶部：ExtremeHills(Edge/M)/IceMountains。
    MixTop {
        mix_top: i32,
        mix_fill: i32,
        width: f64,
        height: f64,
        small_w: f64,
        small_s: f64,
    },
    /// 沙漠：river 命中时铺草，否则 5 层沙 + 3 层沙岩。
    Desert,
    /// 平原/丘陵 + `noise2f > height` 的顶层 mix：Savanna。
    Savanna {
        mix_block: i32,
        width: f64,
        height: f64,
    },
    /// SavannaM：悬崖状态机 + 高原分层（`k > 74` 与 `r` 递减两段）。
    SavannaM {
        min: f64,
        s_cliff: f64,
        s_height: f64,
        s_strength: f64,
        c_cliff: f64,
        mix_block: i32,
    },
    /// 台地条带：SavannaPlateau(M)。
    SavannaPlateau { grass_raise: i32, mix_block: i32 },
    /// Mesa 家族：cliff 时整列直接铺条带。
    Mesa { grass_raise: i32 },
    /// 台地带 mix3/mix4：MesaPlateauF(M)。
    MesaF {
        grass_raise: i32,
        mix3_height: f64,
        mix4_height: f64,
    },
    /// 海洋：`depth==0` 时 mix 噪声决定海床方块，`deep` 版多一段 `k < 69` 的沙。
    Ocean {
        mix_block: i32,
        width: f64,
        height: f64,
        deep: bool,
    },
    /// 沙滩：`k > 63 && 3 < depth < 6` 时铺 mix 方块。
    Beach,
    /// 沼泽：与 Simplified 相同，但悬崖分支要求 `k > 64`。
    Swamp,
    /// 河流：`river` 门槛 + `depth > 4` 提前返回。
    River,
    /// 冰原：river 命中铺石/圆石，其余 9 层雪，水面覆冰。
    Polar,
}

#[derive(Clone, Copy)]
enum CliffBlock {
    Fixed(i32),
    /// `rand.nextInt(3) == 0 ? hcCobble : hcStone`
    StoneOrCobble,
    /// `rand.nextInt(3) == 0 ? 384 : 192`
    SandOrSandstone,
}

impl Surface {
    fn hc_stone(&self) -> i32 {
        self.cliff_stone_block
    }

    fn hc_cobble(&self) -> i32 {
        self.cliff_cobble_block
    }

    fn shadow(&self) -> i32 {
        self.shadow_stone_block
    }
}

/// `SurfaceRegistry::create`：registry 里每个 biome 的构造实参在这里落成字段。
pub fn create(id: i32) -> Surface {
    // `$topBlock` / `$fillerBlock` 的全局默认与特例（顺序同 PHP）。
    let mut top = 32;
    let mut filler = 48;
    if matches!(id, 2 | 17 | 130 | 16 | 26) {
        top = 192;
        filler = 192;
    }
    if id == 25 {
        top = 16;
        filler = 16;
    }
    if id == 140 {
        top = 1280;
    }
    if matches!(id, 14 | 15) {
        top = 1760;
    }
    if matches!(id, 37 | 38 | 39 | 165 | 166 | 167) {
        top = 193;
        filler = 2544;
    }

    let (kind, top, filler) = match id {
        // 简化版族
        1 | 23 | 129 | 149 | 151 | 156 | 157 | 158 => (
            Kind::Simple {
                cliff: 1.399999976158142,
            },
            top,
            filler,
        ),
        // 单层 mix 族
        27 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 50, 0.15000000596046448),
            top,
            filler,
        ),
        28 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 50, 0.10000000149011612),
            top,
            filler,
        ),
        155 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 49, 0.15000000596046448),
            top,
            filler,
        ),
        34 | 162 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 208, 0.07999999821186066),
            top,
            filler,
        ),
        132 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 32, 0.05000000074505806),
            top,
            filler,
        ),
        21 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 50, 0.09000000357627869),
            top,
            filler,
        ),
        29 => (
            mixed(0.0, 1.5, 60.0, 65.0, 1.5, 50, 0.15000000596046448),
            top,
            filler,
        ),
        // 双层 mix 族
        4 | 18 => (
            Kind::Mixed2 {
                min: f(0.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                mix_block: 50,
                mix_height: f(0.6000000238418579),
                mix2_block: 50,
                mix2_height: f(-0.4000000059604645),
            },
            top,
            filler,
        ),
        // 雪地针叶林
        5 | 19 | 30 | 32 | 133 | 160 => (Kind::SnowyMix, 32, 48),
        // 雪地丘陵：registry 只传 4 个实参，其余保持类字段初值
        31 | 33 | 161 => (
            Kind::SnowyCliff {
                min: f(0.20000000298023224),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                i_cliff: f(0.30000001192092896),
                i_height: f(100.0),
                i_strength: f(50.0),
                c_cliff: f(1.5),
            },
            32,
            48,
        ),
        // MushroomIsland(Shore)：悬崖状态机但顶层没有 `k < 63` 分支
        14 | 15 => (
            Kind::CliffState {
                min: f(0.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                top1: CliffBlock::StoneOrCobble,
                top2: SHADOW_STONE,
                below1: CLIFF_STONE,
                below2: SHADOW_STONE,
                k63: false,
            },
            1760,
            48,
        ),
        // JungleHills
        22 => (
            Kind::CliffState {
                min: f(1.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                top1: CliffBlock::StoneOrCobble,
                top2: SHADOW_STONE,
                below1: CLIFF_STONE,
                below2: SHADOW_STONE,
                k63: true,
            },
            32,
            48,
        ),
        // DesertHills：cliff 上下都是沙岩
        17 => (
            Kind::CliffState {
                min: f(0.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                top1: CliffBlock::Fixed(384),
                top2: 384,
                below1: 384,
                below2: 384,
                k63: true,
            },
            192,
            384,
        ),
        // DesertM
        130 => (
            Kind::CliffState {
                min: f(0.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                top1: CliffBlock::SandOrSandstone,
                top2: SHADOW_DESERT,
                below1: 192,
                below2: SHADOW_DESERT,
                k63: true,
            },
            192,
            384,
        ),
        // 沙漠本体
        2 => (Kind::Desert, 192, 192),
        // ExtremeHills(Edge/M)
        3 | 20 | 131 => (
            mix_top(32, 48, 60.0, -0.14000000059604645, 14.0, 0.25),
            32,
            48,
        ),
        // IceMountains
        13 => (
            mix_top(1280, 1280, 60.0, -0.14000000059604645, 14.0, 0.25),
            1280,
            1280,
        ),
        // IcePlainsSpikes：与 Simplified 同构，cliff 方块换成雪块
        140 => (
            Kind::Simple {
                cliff: 1.399999976158142,
            },
            1280,
            48,
        ),
        // Savanna
        35 => (
            Kind::Savanna {
                mix_block: 32,
                width: f(13.0),
                height: f(0.27000001072883606),
            },
            32,
            48,
        ),
        // SavannaM
        163 => (
            Kind::SavannaM {
                min: f(0.0),
                s_cliff: f(1.5),
                s_height: f(60.0),
                s_strength: f(65.0),
                c_cliff: f(1.5),
                mix_block: 49,
            },
            32,
            48,
        ),
        // SavannaPlateau(M)
        36 => (
            Kind::SavannaPlateau {
                grass_raise: 0,
                mix_block: 49,
            },
            32,
            48,
        ),
        // SavannaPlateauM：循环体与 SavannaPlateau 相同，只是地形更陡
        164 => (
            Kind::SavannaPlateau {
                grass_raise: 0,
                mix_block: 49,
            },
            32,
            48,
        ),
        // Mesa 家族：filler 一律是 `(2544 | 1)`
        37 | 165 | 39 | 167 => (Kind::Mesa { grass_raise: 0 }, 193, 2545),
        38 | 166 => (
            Kind::MesaF {
                grass_raise: 0,
                mix3_height: f(0.20000000298023224),
                mix4_height: f(0.6000000238418579),
            },
            193,
            2545,
        ),
        // 海洋
        0 | 10 => (ocean(208, 20.0, 0.20000000298023224, false), 192, 192),
        24 => (ocean(1312, 20.0, 0.10000000149011612, true), 208, 208),
        // 沙滩
        16 | 26 => (Kind::Beach, 192, 192),
        25 => (Kind::Beach, 208, 208),
        // 沼泽
        6 | 134 => (Kind::Swamp, 32, 48),
        // 河流
        7 | 11 => (Kind::River, 32, 48),
        // 冰原
        12 => (Kind::Polar, 1280, 48),
        _ => panic!("surface biome {id} missing from SurfaceRegistry port"),
    };

    // 只有这两个 biome 覆盖 `hcStone()/hcCobble()`
    let (cliff_stone, cliff_cobble) = match id {
        13 => (1280, 2784),
        140 => (1280, 1280),
        _ => (CLIFF_STONE, CLIFF_COBBLE),
    };

    Surface {
        biome_id: id,
        top_block: top,
        filler_block: filler,
        shadow_stone_block: SHADOW_STONE,
        shadow_desert_block: SHADOW_DESERT,
        cliff_stone_block: cliff_stone,
        cliff_cobble_block: cliff_cobble,
        kind,
    }
}

fn mix_top(
    mix_top: i32,
    mix_fill: i32,
    width: f64,
    height: f64,
    small_w: f64,
    small_s: f64,
) -> Kind {
    Kind::MixTop {
        mix_top,
        mix_fill,
        width: f(width),
        height: f(height),
        small_w: f(small_w),
        small_s: f(small_s),
    }
}

fn ocean(mix_block: i32, width: f64, height: f64, deep: bool) -> Kind {
    Kind::Ocean {
        mix_block,
        width: f(width),
        height: f(height),
        deep,
    }
}

fn mixed(
    min: f64,
    s_cliff: f64,
    s_height: f64,
    s_strength: f64,
    c_cliff: f64,
    mix_block: i32,
    mix_height: f64,
) -> Kind {
    Kind::Mixed {
        min: f(min),
        s_cliff: f(s_cliff),
        s_height: f(s_height),
        s_strength: f(s_strength),
        c_cliff: f(c_cliff),
        mix_block,
        mix_height: f(mix_height),
    }
}

impl Surface {
    /// `paintTerrain($primer, $i, $j, $x, $z, -1, $rtgWorld, $noise, $river, $base)`。
    pub fn paint_terrain(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        match &self.kind {
            Kind::Simple { cliff } => {
                self.paint_simple(primer, x, z, depth, world, noise, river, *cliff)
            }
            Kind::Mixed { .. } => self.paint_mixed(primer, i, j, x, z, depth, world, noise, river),
            Kind::Mixed2 { .. } => {
                self.paint_mixed2(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::SnowyMix => self.paint_snowy_mix(primer, i, j, x, z, depth, world, noise, river),
            Kind::SnowyCliff { .. } => {
                self.paint_snowy_cliff(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::CliffState { .. } => {
                self.paint_cliff_state(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::MixTop { .. } => {
                self.paint_mix_top(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::Desert => self.paint_desert(primer, i, j, x, z, depth, world, river),
            Kind::Savanna { .. } => {
                self.paint_savanna(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::SavannaM { .. } => {
                self.paint_savanna_m(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::SavannaPlateau { .. } => {
                self.paint_savanna_plateau(primer, x, z, depth, world, noise, river)
            }
            Kind::Mesa { .. } | Kind::MesaF { .. } => {
                self.paint_mesa(primer, i, j, x, z, depth, world, noise, river)
            }
            Kind::Ocean { .. } => self.paint_ocean(primer, i, j, x, z, depth, world),
            Kind::Beach => self.paint_beach(primer, x, z, depth),
            Kind::Swamp => self.paint_swamp(primer, x, z, depth, world, noise, river),
            Kind::River => self.paint_river(primer, i, j, x, z, depth, world, river),
            Kind::Polar => self.paint_polar(primer, i, j, x, z, depth, world, noise, river),
        }
    }

    fn paint_simple(
        &self,
        primer: &mut ChunkPrimer,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
        cliff_threshold: f64,
    ) {
        let mut rand = world.rand();
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > cliff_threshold;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff {
                    if depth > -1 && depth < 2 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 10 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    }
                } else if depth == 0 && k > 61 {
                    primer.set_block_state(x, k, z, self.top_block);
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    fn paint_mixed(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::Mixed {
            min,
            s_cliff,
            s_height,
            s_strength,
            c_cliff,
            mix_block,
            mix_height,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 {
                    let p = f(simplex.noise3f(
                        f(f(i as f64) / 8.0) as f32,
                        f(f(j as f64) / 8.0) as f32,
                        f(f(k as f64) / 8.0) as f32,
                    ) as f64
                        * 0.5);

                    if c > min && c > f(f(s_cliff - f(f(k as f64) - s_height) / s_strength) + p) {
                        cliff = 1;
                    }
                    if c > c_cliff {
                        cliff = 2;
                    }

                    if cliff == 1 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if k < 63 {
                        let block = if k < 62 {
                            self.filler_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if f(simplex
                        .noise2f(f(f(i as f64) / 12.0) as f32, f(f(j as f64) / 12.0) as f32)
                        as f64)
                        > mix_height
                    {
                        primer.set_block_state(x, k, z, mix_block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 6 {
                    if cliff == 1 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                }
            }
            k -= 1;
        }
    }

    fn paint_mixed2(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::Mixed2 {
            min,
            s_cliff,
            s_height,
            s_strength,
            c_cliff,
            mix_block,
            mix_height,
            mix2_block,
            mix2_height,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 {
                    let p = f(simplex.noise3f(
                        f(f(i as f64) / 8.0) as f32,
                        f(f(j as f64) / 8.0) as f32,
                        f(f(k as f64) / 8.0) as f32,
                    ) as f64
                        * 0.5);

                    if c > min && c > f(f(s_cliff - f(f(k as f64) - s_height) / s_strength) + p) {
                        cliff = 1;
                    }
                    if c > c_cliff {
                        cliff = 2;
                    }

                    if cliff == 1 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if k < 63 {
                        let block = if k < 62 {
                            self.filler_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else {
                        let mix_noise = f(simplex
                            .noise2f(f(f(i as f64) / 12.0) as f32, f(f(j as f64) / 12.0) as f32)
                            as f64);
                        if mix_noise < mix2_height {
                            primer.set_block_state(x, k, z, mix2_block);
                        } else if mix_noise > mix_height {
                            primer.set_block_state(x, k, z, mix_block);
                        } else {
                            primer.set_block_state(x, k, z, self.top_block);
                        }
                    }
                } else if depth < 6 {
                    if cliff == 1 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                }
            }
            k -= 1;
        }
    }

    fn paint_snowy_mix(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let p = f(
            simplex.noise2f(f(f(i as f64) / 8.0) as f32, f(f(j as f64) / 8.0) as f32) as f64 * 0.5,
        );
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 {
                    if c > 0.44999998807907104 && c > f(f(1.5 - f(f(k as f64) - 60.0) / 65.0) + p) {
                        cliff = 1;
                    }
                    if c > 1.5 {
                        cliff = 2;
                    }
                    if f(k as f64) > f(110.0 + f(p * 4.0))
                        && c < f(f(0.30000001192092896 + f(f(k as f64) - 100.0) / 50.0) + p)
                    {
                        cliff = 3;
                    }

                    if cliff == 1 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if cliff == 3 {
                        primer.set_block_state(x, k, z, 1280);
                    } else if f(simplex
                        .noise2f(f(f(i as f64) / 50.0) as f32, f(f(j as f64) / 50.0) as f32)
                        as f64
                        + f(p * 0.6000000238418579))
                        > 0.23999999463558197
                    {
                        primer.set_block_state(x, k, z, 50);
                    } else {
                        primer.set_block_state(x, k, z, 32);
                    }
                } else if depth < 6 {
                    if cliff == 1 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if cliff == 3 {
                        primer.set_block_state(x, k, z, 1280);
                    } else {
                        primer.set_block_state(x, k, z, 48);
                    }
                }
            }
            k -= 1;
        }
    }

    fn paint_snowy_cliff(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::SnowyCliff {
            min,
            s_cliff,
            s_height,
            s_strength,
            i_cliff,
            i_height,
            i_strength,
            c_cliff,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 {
                    let p = f(simplex.noise3f(
                        f(f(i as f64) / 8.0) as f32,
                        f(f(j as f64) / 8.0) as f32,
                        f(f(k as f64) / 8.0) as f32,
                    ) as f64
                        * 0.5);

                    if c > min && c > f(f(s_cliff - f(f(k as f64) - s_height) / s_strength) + p) {
                        cliff = 1;
                    }
                    if c > c_cliff {
                        cliff = 2;
                    }
                    if f(k as f64) > f(110.0 + f(p * 4.0))
                        && c < f(f(i_cliff + f(f(k as f64) - i_height) / i_strength) + p)
                    {
                        cliff = 3;
                    }

                    if cliff == 1 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if cliff == 3 {
                        primer.set_block_state(x, k, z, 1280);
                    } else if k < 63 {
                        let block = if k < 62 {
                            self.filler_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else {
                        primer.set_block_state(x, k, z, 32);
                    }
                } else if depth < 6 {
                    if cliff == 1 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, self.shadow());
                    } else if cliff == 3 {
                        primer.set_block_state(x, k, z, 1280);
                    } else {
                        primer.set_block_state(x, k, z, 48);
                    }
                }
            }
            k -= 1;
        }
    }

    fn paint_cliff_state(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::CliffState {
            min,
            s_cliff,
            s_height,
            s_strength,
            c_cliff,
            top1,
            top2,
            below1,
            below2,
            k63,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 {
                    let p = f(simplex.noise3f(
                        f(f(i as f64) / 8.0) as f32,
                        f(f(j as f64) / 8.0) as f32,
                        f(f(k as f64) / 8.0) as f32,
                    ) as f64
                        * 0.5);

                    if c > min && c > f(f(s_cliff - f(f(k as f64) - s_height) / s_strength) + p) {
                        cliff = 1;
                    }
                    if c > c_cliff {
                        cliff = 2;
                    }

                    if cliff == 1 {
                        let block = match top1 {
                            CliffBlock::Fixed(block) => block,
                            CliffBlock::StoneOrCobble => {
                                if rand.next_int_bound(3) == 0 {
                                    self.hc_cobble()
                                } else {
                                    self.hc_stone()
                                }
                            }
                            CliffBlock::SandOrSandstone => {
                                if rand.next_int_bound(3) == 0 {
                                    384
                                } else {
                                    192
                                }
                            }
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, top2);
                    } else if k63 && k < 63 {
                        let block = if k < 62 {
                            self.filler_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 6 {
                    if cliff == 1 {
                        primer.set_block_state(x, k, z, below1);
                    } else if cliff == 2 {
                        primer.set_block_state(x, k, z, below2);
                    } else {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                }
            }
            k -= 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_mix_top(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::MixTop {
            mix_top,
            mix_fill,
            width,
            height,
            small_w,
            small_s,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > 1.399999976158142;
        let mut mix = false;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff {
                    if depth > -1 && depth < 2 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 10 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    }
                } else if depth == 0 && k > 61 {
                    let big = simplex
                        .noise2f(f(f(i as f64) / width) as f32, f(f(j as f64) / width) as f32)
                        as f64;
                    let small = simplex.noise2f(
                        f(f(i as f64) / small_w) as f32,
                        f(f(j as f64) / small_w) as f32,
                    ) as f64;
                    if f(big + f(small * small_s)) > height {
                        primer.set_block_state(x, k, z, mix_top);
                        mix = true;
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 4 {
                    let block = if mix { mix_fill } else { self.filler_block };
                    primer.set_block_state(x, k, z, block);
                }
            }
            k -= 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_desert(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        river: f64,
    ) {
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let mut river_paint = false;
        let mut grass = false;

        if river > 0.05000000074505806
            && f(river
                + f(
                    simplex.noise2f(f(f(i as f64) / 10.0) as f32, f(f(j as f64) / 10.0) as f32)
                        as f64
                        * 0.10000000149011612,
                ))
                > 0.8600000143051147
        {
            river_paint = true;
            if simplex.noise2f(f(f(i as f64) / 12.0) as f32, f(f(j as f64) / 12.0) as f32) as f64
                > 0.25
            {
                grass = true;
            }
        }

        let mut depth = depth;
        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if river_paint {
                    if grass && depth < 4 {
                        primer.set_block_state(x, k, z, self.filler_block);
                    } else if depth == 0 {
                        let block = if rand.next_int_bound(2) == 0 {
                            self.top_block
                        } else {
                            384
                        };
                        primer.set_block_state(x, k, z, block);
                    }
                } else if depth > -1 && depth < 5 {
                    primer.set_block_state(x, k, z, self.top_block);
                } else if depth < 8 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    fn paint_savanna(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::Savanna {
            mix_block,
            width,
            height,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > 1.399999976158142;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff {
                    if depth > -1 && depth < 2 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 10 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    }
                } else if depth == 0 && k > 61 {
                    let mix_check = simplex
                        .noise2f(f(f(i as f64) / width) as f32, f(f(j as f64) / width) as f32)
                        as f64;
                    if mix_check > height {
                        primer.set_block_state(x, k, z, mix_block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    fn paint_savanna_m(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::SavannaM {
            min,
            s_cliff,
            s_height,
            s_strength,
            c_cliff,
            mix_block,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let mut cliff = 0;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                let p = f(simplex.noise3f(
                    f(f(i as f64) / 8.0) as f32,
                    f(f(j as f64) / 8.0) as f32,
                    f(f(k as f64) / 8.0) as f32,
                ) as f64
                    * 0.5);

                if c > min && c > f(f(s_cliff - f(f(k as f64) - s_height) / s_strength) + p) {
                    cliff = 1;
                }
                if c > c_cliff {
                    cliff = 2;
                }

                if cliff == 1 {
                    let block = if rand.next_int_bound(3) == 0 {
                        self.hc_cobble()
                    } else {
                        self.hc_stone()
                    };
                    primer.set_block_state(x, k, z, block);
                } else if cliff == 2 {
                    if depth > -1 && depth < 2 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 10 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    }
                } else if k > 74 {
                    if depth == 0 {
                        let block = if rand.next_int_bound(5) == 0 {
                            mix_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 4 {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                } else if depth == 0 && k > 61 {
                    let r = f(f((k - 62) as f64) / 2.0) as i32;
                    if rand.next_int_bound(r + 2) == 0 {
                        primer.set_block_state(x, k, z, 32);
                    } else if rand.next_int_bound(f(f(r as f64) / 2.0) as i32 + 2) == 0 {
                        primer.set_block_state(x, k, z, mix_block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    fn paint_savanna_plateau(
        &self,
        primer: &mut ChunkPrimer,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let Kind::SavannaPlateau {
            grass_raise,
            mix_block,
        } = self.kind
        else {
            unreachable!()
        };
        let mut rand = world.rand();
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > 1.2999999523162842;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff {
                    primer.set_block_state(x, k, z, plateau_band(self.biome_id, k));
                } else if k > 74 + grass_raise {
                    if depth == 0 {
                        let block = if rand.next_int_bound(5) == 0 {
                            mix_block
                        } else {
                            self.top_block
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 4 {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                } else if depth == 0 && k > 61 {
                    let r = f(f((k - (62 + grass_raise)) as f64) / 2.0) as i32;
                    if rand.next_int_bound(r + 2) == 0 {
                        primer.set_block_state(x, k, z, self.top_block);
                    } else if rand.next_int_bound(f(f(r as f64) / 2.0) as i32 + 2) == 0 {
                        primer.set_block_state(x, k, z, mix_block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_mesa(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let (grass_raise, mix3_height, mix4_height) = match self.kind {
            Kind::Mesa { grass_raise } => (grass_raise, 0.0, 0.0),
            Kind::MesaF {
                grass_raise,
                mix3_height,
                mix4_height,
            } => (grass_raise, mix3_height, mix4_height),
            _ => unreachable!(),
        };
        let is_f = matches!(self.kind, Kind::MesaF { .. });
        let mut rand = world.rand();
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > 1.2999999523162842;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff {
                    primer.set_block_state(x, k, z, plateau_band(self.biome_id, k));
                } else {
                    // MesaPlateauF 的 mix 噪声每列算一次，PHP 放在循环里但无副作用
                    let mix_noise = if is_f {
                        let simplex = world.simplex_instance(0);
                        f(simplex
                            .noise2f(f(f(i as f64) / 12.0) as f32, f(f(j as f64) / 12.0) as f32)
                            as f64)
                    } else {
                        0.0
                    };

                    if k > 74 + grass_raise {
                        if depth == 0 {
                            if is_f && mix_noise > mix4_height {
                                primer.set_block_state(x, k, z, 32);
                            } else if is_f && mix_noise > mix3_height {
                                primer.set_block_state(x, k, z, 49);
                            } else {
                                let block = if rand.next_int_bound(5) == 0 {
                                    2864
                                } else {
                                    self.top_block
                                };
                                primer.set_block_state(x, k, z, block);
                            }
                        } else if depth < 4 {
                            primer.set_block_state(x, k, z, self.filler_block);
                        }
                    } else if depth == 0 && k > 61 {
                        let r = f(f((k - (62 + grass_raise)) as f64) / 2.0) as i32;
                        if rand.next_int_bound(r + 2) == 0 {
                            primer.set_block_state(x, k, z, 2545);
                        } else if rand.next_int_bound(f(f(r as f64) / 2.0) as i32 + 2) == 0 {
                            primer.set_block_state(x, k, z, 2864);
                        } else {
                            primer.set_block_state(x, k, z, self.top_block);
                        }
                    } else if depth < 4 {
                        primer.set_block_state(x, k, z, self.filler_block);
                    }
                }
            }
            k -= 1;
        }
    }

    fn paint_ocean(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
    ) {
        let Kind::Ocean {
            mix_block,
            width,
            height,
            deep,
        } = self.kind
        else {
            unreachable!()
        };
        let simplex = world.simplex_instance(0);
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 && k > 0 && k < 63 {
                    let mix_check = f(simplex
                        .noise2f(f(f(i as f64) / width) as f32, f(f(j as f64) / width) as f32)
                        as f64);
                    if mix_check > height {
                        primer.set_block_state(x, k, z, mix_block);
                    } else {
                        primer.set_block_state(x, k, z, self.top_block);
                    }
                } else if depth < 4 && k < 63 {
                    primer.set_block_state(x, k, z, self.filler_block);
                } else if !deep && depth == 0 && k < 69 {
                    primer.set_block_state(x, k, z, 192);
                }
            }
            k -= 1;
        }
    }

    fn paint_beach(&self, primer: &mut ChunkPrimer, x: i32, z: i32, depth: i32) {
        let mut depth = depth;
        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if depth == 0 && k > 61 {
                    primer.set_block_state(x, k, z, self.top_block);
                } else if k > 63 && depth > 3 && depth < 6 {
                    primer.set_block_state(x, k, z, 208);
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_swamp(
        &self,
        primer: &mut ChunkPrimer,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let mut rand = world.rand();
        let c = f(calc_cliff(x as usize, z as usize, noise, river));
        let cliff = c > 1.399999976158142;
        let mut depth = depth;

        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if cliff && k > 64 {
                    if depth > -1 && depth < 2 {
                        let block = if rand.next_int_bound(3) == 0 {
                            self.hc_cobble()
                        } else {
                            self.hc_stone()
                        };
                        primer.set_block_state(x, k, z, block);
                    } else if depth < 10 {
                        primer.set_block_state(x, k, z, self.hc_stone());
                    }
                } else if depth == 0 && k > 61 {
                    primer.set_block_state(x, k, z, self.top_block);
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, self.filler_block);
                }
            }
            k -= 1;
        }
    }

    fn paint_river(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        river: f64,
    ) {
        let simplex = world.simplex_instance(0);
        if river > 0.05000000074505806
            && f(river
                + f(
                    simplex.noise2f(f(f(i as f64) / 10.0) as f32, f(f(j as f64) / 10.0) as f32)
                        as f64
                        * 0.15000000596046448,
                ))
                > 0.800000011920929
        {
            let mut depth = depth;
            let mut k = primer.find_ground_block_idx(x, z) + 1;
            while k >= 0 {
                let b = primer.get_block_state(x, k, z) >> 4;
                if b == 0 {
                    depth = -1;
                } else if b != 9 {
                    depth += 1;
                    if depth == 0 && k > 61 {
                        primer.set_block_state(x, k, z, 32);
                    } else if depth < 4 {
                        primer.set_block_state(x, k, z, 48);
                    } else if depth > 4 {
                        return;
                    }
                }
                k -= 1;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_polar(
        &self,
        primer: &mut ChunkPrimer,
        i: i32,
        j: i32,
        x: i32,
        z: i32,
        depth: i32,
        world: &RtgWorld,
        noise: &[f64],
        river: f64,
    ) {
        let mut rand = world.rand();
        let simplex = world.simplex_instance(0);
        let mut water = false;
        let mut river_paint = false;
        let mut grass = false;

        if river > 0.05000000074505806
            && f(river
                + f(
                    simplex.noise2f(f(f(i as f64) / 10.0) as f32, f(f(j as f64) / 10.0) as f32)
                        as f64
                        * 0.10000000149011612,
                ))
                > 0.8600000143051147
        {
            river_paint = true;
            if simplex.noise2f(f(f(i as f64) / 12.0) as f32, f(f(j as f64) / 12.0) as f32) as f64
                > 0.25
            {
                grass = true;
            }
        }

        let mut depth = depth;
        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if b == 1 {
                depth += 1;
                if river_paint {
                    if grass && depth < 4 {
                        primer.set_block_state(x, k, z, self.filler_block);
                    } else if depth == 0 {
                        let block = if rand.next_int_bound(2) == 0 {
                            self.hc_stone()
                        } else {
                            self.hc_cobble()
                        };
                        primer.set_block_state(x, k, z, block);
                    }
                } else if depth > -1 && depth < 9 {
                    primer.set_block_state(x, k, z, self.top_block);
                    if depth == 0 && k > 61 && k < 254 {
                        calc_snow_height(x, k, z, primer, noise);
                    }
                }
            } else if !water && b == 9 {
                primer.set_block_state(x, k, z, 1264);
                water = true;
            }
            k -= 1;
        }
    }
}

/// `SurfaceVanillaRiver` 的 `depth > 4` 不算完：`GenLayer`/`SurfaceRiverOasis` 是独立类，
/// 由 `TerrainGenerator` 在河流 biome 上额外调用一次。
#[allow(clippy::too_many_arguments)]
pub fn paint_river_oasis(
    primer: &mut ChunkPrimer,
    i: i32,
    j: i32,
    x: i32,
    z: i32,
    depth: i32,
    world: &RtgWorld,
    river: f64,
) {
    let settings = world.get_generator_settings();
    let cut_off_scale = f(settings.river_cut_off_scale);
    let cut_off_ampl = f(settings.river_cut_off_ampl);

    let mut highest_y = 255;
    while highest_y > 0 {
        if primer.get_block_state(x, highest_y, z) >> 4 != 0 {
            break;
        }
        highest_y -= 1;
    }

    let amplitude = f(0.05000000074505806);
    let sim0 = world.simplex_instance(0);
    let sim1 = world.simplex_instance(1);
    let sim2 = world.simplex_instance(2);

    let mut noise_value = f(f(f(sim0
        .noise2f(f(f(i as f64) / 8.0) as f32, f(f(j as f64) / 8.0) as f32)
        as f64
        * amplitude)
        / 1.0)
        + f(f(
            sim1.noise2f(f(f(i as f64) / 3.0) as f32, f(f(j as f64) / 3.0) as f32) as f64
                * amplitude,
        ) / 2.0));

    let noise_neg = f(sim2.noise2f(
        f(f(i as f64) / cut_off_scale) as f32,
        f(f(j as f64) / cut_off_scale) as f32,
    ) as f64
        * cut_off_ampl);
    noise_value = f(noise_value - noise_neg);

    if highest_y > 62 {
        noise_value = f(noise_value - f(f((highest_y - 62) as f64) * f(f(1.0) / 12.0)));
    }

    if river > 0.699999988079071 && f(river + noise_value) > 0.8500000238418579 {
        let mut depth = depth;
        let mut k = primer.find_ground_block_idx(x, z) + 1;
        while k >= 0 {
            let b = primer.get_block_state(x, k, z) >> 4;
            if b == 0 {
                depth = -1;
            } else if !(b == 8 || b == 9) {
                depth += 1;
                if depth == 0 && k > 61 {
                    primer.set_block_state(x, k, z, 32);
                } else if depth < 4 {
                    primer.set_block_state(x, k, z, 48);
                } else if depth > 4 {
                    return;
                }
            }
            k -= 1;
        }
    }
}

/// `SurfaceBase::plateauBand`：台地每层的带子方块。
fn plateau_band(biome: i32, y: i32) -> i32 {
    const MESA: [i32; 33] = [
        2548, 2548, 2548, 2752, 2752, 2545, 2558, 2558, 2752, 2552, 2544, 2752, 2752, 2752, 2545,
        2752, 2752, 2752, 2545, 2552, 2544, 2556, 2556, 2556, 2752, 2545, 2752, 2545, 2752, 2545,
        2752, 2752, 2752,
    ];
    const SAVANNA: [i32; 21] = [
        2544, 2544, 2544, 2544, 2552, 2552, 2556, 2556, 2552, 2544, 2552, 2556, 2556, 2552, 2556,
        2552, 2544, 2544, 2552, 2556, 2556,
    ];
    let bands: &[i32] = if biome == 36 || biome == 164 {
        &SAVANNA
    } else {
        &MESA
    };
    bands[(y % bands.len() as i32) as usize]
}

/// `SurfaceBase::calcSnowHeight`：雪原上按噪声高度堆雪层。
fn calc_snow_height(x: i32, y: i32, z: i32, primer: &mut ChunkPrimer, noise: &[f64]) {
    if y >= 254 {
        return;
    }
    let n = noise[(x * 16 + z) as usize];
    let h = f(f(n - (n as i32) as f64) * 8.0) as i32;
    if h > 7 {
        primer.set_block_state(x, y + 2, z, 1248);
        primer.set_block_state(x, y + 1, z, 1254);
    } else if h > 0 {
        primer.set_block_state(x, y + 1, z, 1248 | (h - 1));
    }
}
