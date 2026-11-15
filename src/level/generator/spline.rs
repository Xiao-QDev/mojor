// 移植自 net.minecraft.data.worldgen.TerrainProvider
// 三个输入：continents / erosion / ridges (peaks_and_valleys_folded)
// 输出 offset（高度偏移，约 -0.5..1.0）和 factor（起伏乘数，约 1..10）
// 只保留 overworldOffset 和 overworldFactor 两条样条 jaggedness 不用（无 3D 噪声）

// --- 节点树 ----------------------------------------------------------------

#[derive(Clone)]
pub struct SplineNode {
    pub loc: f32,
    pub val: SplineTree,
    pub der: f32,
}

#[derive(Clone, Copy)]
pub enum Axis {
    Continents,
    Erosion,
    Ridges,
}

#[derive(Clone)]
pub enum SplineTree {
    Const(f64),
    Spline { axis: Axis, nodes: Vec<SplineNode> },
}

impl SplineTree {
    pub fn eval(&self, c: f64, e: f64, r: f64) -> f64 {
        match self {
            SplineTree::Const(v) => *v,
            SplineTree::Spline { axis, nodes } => {
                let x = match axis {
                    Axis::Continents => c as f32,
                    Axis::Erosion => e as f32,
                    Axis::Ridges => r as f32,
                };
                eval_spline(nodes, x, c, e, r)
            }
        }
    }
}

// --- 三次 Hermite 插值（与 Vanilla CubicSpline 相同）---------------------

fn cubic_hermite(t: f64, p0: f64, p1: f64, m0: f64, m1: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    (2.0 * t3 - 3.0 * t2 + 1.0) * p0
        + (t3 - 2.0 * t2 + t) * m0
        + (-2.0 * t3 + 3.0 * t2) * p1
        + (t3 - t2) * m1
}

fn eval_spline(nodes: &[SplineNode], x: f32, c: f64, e: f64, r: f64) -> f64 {
    if nodes.is_empty() {
        return 0.0;
    }
    if x <= nodes[0].loc {
        let y = nodes[0].val.eval(c, e, r);
        return y + nodes[0].der as f64 * (x - nodes[0].loc) as f64;
    }
    let last = &nodes[nodes.len() - 1];
    if x >= last.loc {
        let y = last.val.eval(c, e, r);
        return y + last.der as f64 * (x - last.loc) as f64;
    }
    let idx = nodes.partition_point(|n| n.loc < x) - 1;
    let n0 = &nodes[idx];
    let n1 = &nodes[idx + 1];
    let y0 = n0.val.eval(c, e, r);
    let y1 = n1.val.eval(c, e, r);
    let dx = (n1.loc - n0.loc) as f64;
    let t = (x as f64 - n0.loc as f64) / dx;
    let m0 = n0.der as f64 * dx;
    let m1 = n1.der as f64 * dx;
    cubic_hermite(t, y0, y1, m0, m1)
}

// --- TerrainProvider 辅助函数 ---------------------------------------------

fn lerp_f32(t: f32, a: f32, b: f32) -> f32 {
    a + t * (b - a)
}

pub fn peaks_and_valleys(weirdness: f32) -> f32 {
    -((weirdness.abs() - 0.666_666_7).abs() - 0.333_333_34) * 3.0
}

fn mountain_continentalness(ridge: f32, modulation: f32, allow_rivers_below: f32) -> f32 {
    let ridge_slope = 1.0 - (1.0 - modulation) * 0.5;
    let ridge_intersect = 0.5 * (1.0 - modulation);
    let adjusted = (ridge + 1.17) * 0.460_829_47;
    let cont = adjusted * ridge_slope - ridge_intersect;
    if ridge < allow_rivers_below {
        cont.max(-0.222_2)
    } else {
        cont.max(0.0)
    }
}

fn calc_mountain_ridge_zero(modulation: f32) -> f32 {
    let ridge_slope = 1.0 - (1.0 - modulation) * 0.5;
    let ridge_intersect = 0.5 * (1.0 - modulation);
    ridge_intersect / (0.460_829_47 * ridge_slope) - 1.17
}

fn slope(y1: f32, y2: f32, x1: f32, x2: f32) -> f32 {
    (y2 - y1) / (x2 - x1)
}

// 对应 ridgeSpline
fn ridge_spline(
    valley: f32,
    low: f32,
    mid: f32,
    high: f32,
    peaks: f32,
    min_valley_steepness: f32,
) -> SplineTree {
    let d1 = (0.5 * (low - valley)).max(min_valley_steepness);
    let d2 = 5.0 * (mid - low);
    SplineTree::Spline {
        axis: Axis::Ridges,
        nodes: vec![
            SplineNode { loc: -1.0, val: SplineTree::Const(valley as f64), der: d1 },
            SplineNode { loc: -0.4, val: SplineTree::Const(low as f64), der: d1.min(d2) },
            SplineNode { loc: 0.0, val: SplineTree::Const(mid as f64), der: d2 },
            SplineNode { loc: 0.4, val: SplineTree::Const(high as f64), der: 2.0 * (high - mid) },
            SplineNode {
                loc: 1.0,
                val: SplineTree::Const(peaks as f64),
                der: 0.7 * (peaks - high),
            },
        ],
    }
}

// 对应 buildMountainRidgeSplineWithPoints
fn build_mountain_ridge_spline(modulation: f32, saddle: bool) -> SplineTree {
    let allow = -0.7_f32;
    let min = mountain_continentalness(-1.0, modulation, allow);
    let max = mountain_continentalness(1.0, modulation, allow);
    let zero = calc_mountain_ridge_zero(modulation);

    if (-0.65..1.0).contains(&zero) {
        let after = mountain_continentalness(-0.65, modulation, allow);
        let before = mountain_continentalness(-0.75, modulation, allow);
        let d_min = slope(min, before, -1.0, -0.75);
        let zero_cont = mountain_continentalness(zero, modulation, allow);
        let d_max = slope(zero_cont, max, zero, 1.0);
        SplineTree::Spline {
            axis: Axis::Ridges,
            nodes: vec![
                SplineNode { loc: -1.0, val: SplineTree::Const(min as f64), der: d_min },
                SplineNode { loc: -0.75, val: SplineTree::Const(before as f64), der: 0.0 },
                SplineNode { loc: -0.65, val: SplineTree::Const(after as f64), der: 0.0 },
                SplineNode {
                    loc: zero - 0.01,
                    val: SplineTree::Const(zero_cont as f64),
                    der: 0.0,
                },
                SplineNode {
                    loc: zero,
                    val: SplineTree::Const(zero_cont as f64),
                    der: d_max,
                },
                SplineNode { loc: 1.0, val: SplineTree::Const(max as f64), der: d_max },
            ],
        }
    } else {
        let d = slope(min, max, -1.0, 1.0);
        if saddle {
            SplineTree::Spline {
                axis: Axis::Ridges,
                nodes: vec![
                    SplineNode {
                        loc: -1.0,
                        val: SplineTree::Const(0.2_f32.max(min) as f64),
                        der: 0.0,
                    },
                    SplineNode {
                        loc: 0.0,
                        val: SplineTree::Const(lerp_f32(0.5, min, max) as f64),
                        der: d,
                    },
                    SplineNode { loc: 1.0, val: SplineTree::Const(max as f64), der: d },
                ],
            }
        } else {
            SplineTree::Spline {
                axis: Axis::Ridges,
                nodes: vec![
                    SplineNode { loc: -1.0, val: SplineTree::Const(min as f64), der: d },
                    SplineNode { loc: 1.0, val: SplineTree::Const(max as f64), der: d },
                ],
            }
        }
    }
}

// 对应 getErosionFactor
// factor 样条里针对 erosion 轴的子样条
fn get_erosion_factor(base_value: f32, shattered: bool) -> SplineTree {
    let base = SplineTree::Spline {
        axis: Axis::Ridges,
        nodes: vec![
            SplineNode { loc: -0.2, val: SplineTree::Const(6.3), der: 0.0 },
            SplineNode { loc: 0.2, val: SplineTree::Const(base_value as f64), der: 0.0 },
        ],
    };

    let mut nodes: Vec<SplineNode> = vec![
        SplineNode { loc: -0.6, val: base.clone(), der: 0.0 },
        SplineNode {
            loc: -0.5,
            val: SplineTree::Spline {
                axis: Axis::Ridges,
                nodes: vec![
                    SplineNode { loc: -0.05, val: SplineTree::Const(6.3), der: 0.0 },
                    SplineNode { loc: 0.05, val: SplineTree::Const(2.67), der: 0.0 },
                ],
            },
            der: 0.0,
        },
        SplineNode { loc: -0.35, val: base.clone(), der: 0.0 },
        SplineNode { loc: -0.25, val: base.clone(), der: 0.0 },
        SplineNode {
            loc: -0.1,
            val: SplineTree::Spline {
                axis: Axis::Ridges,
                nodes: vec![
                    SplineNode { loc: -0.05, val: SplineTree::Const(2.67), der: 0.0 },
                    SplineNode { loc: 0.05, val: SplineTree::Const(6.3), der: 0.0 },
                ],
            },
            der: 0.0,
        },
        SplineNode { loc: 0.03, val: base.clone(), der: 0.0 },
    ];

    if shattered {
        let weirdness_shattered = SplineTree::Spline {
            axis: Axis::Ridges,
            nodes: vec![
                SplineNode { loc: 0.0, val: SplineTree::Const(base_value as f64), der: 0.0 },
                SplineNode { loc: 0.1, val: SplineTree::Const(0.625), der: 0.0 },
            ],
        };
        let ridges_shattered = SplineTree::Spline {
            axis: Axis::Ridges,
            nodes: vec![
                SplineNode { loc: -0.9, val: SplineTree::Const(base_value as f64), der: 0.0 },
                SplineNode { loc: -0.69, val: weirdness_shattered, der: 0.0 },
            ],
        };
        nodes.push(SplineNode {
            loc: 0.35,
            val: SplineTree::Const(base_value as f64),
            der: 0.0,
        });
        nodes.push(SplineNode { loc: 0.45, val: ridges_shattered.clone(), der: 0.0 });
        nodes.push(SplineNode { loc: 0.55, val: ridges_shattered, der: 0.0 });
        nodes.push(SplineNode {
            loc: 0.62,
            val: SplineTree::Const(base_value as f64),
            der: 0.0,
        });
    } else {
        let extreme_hills = SplineTree::Spline {
            axis: Axis::Ridges,
            nodes: vec![
                SplineNode { loc: -0.7, val: base.clone(), der: 0.0 },
                SplineNode { loc: -0.15, val: SplineTree::Const(1.37), der: 0.0 },
            ],
        };
        let peaks_only = SplineTree::Spline {
            axis: Axis::Ridges,
            nodes: vec![
                SplineNode { loc: 0.45, val: base, der: 0.0 },
                SplineNode { loc: 0.7, val: SplineTree::Const(1.56), der: 0.0 },
            ],
        };
        nodes.push(SplineNode { loc: 0.05, val: peaks_only.clone(), der: 0.0 });
        nodes.push(SplineNode { loc: 0.4, val: peaks_only, der: 0.0 });
        nodes.push(SplineNode { loc: 0.45, val: extreme_hills.clone(), der: 0.0 });
        nodes.push(SplineNode { loc: 0.55, val: extreme_hills, der: 0.0 });
        nodes.push(SplineNode {
            loc: 0.58,
            val: SplineTree::Const(base_value as f64),
            der: 0.0,
        });
    }

    SplineTree::Spline { axis: Axis::Erosion, nodes }
}

// 对应 buildErosionOffsetSpline
// 参数顺序与 Java 一致:
//   lowValley, hill, tallHill, mountainFactor, plain, swamp, includeExtremeHills, saddle
fn build_erosion_offset_spline(
    low_valley: f32,
    hill: f32,
    tall_hill: f32,
    mountain_factor: f32,
    plain: f32,
    swamp: f32,
    include_extreme_hills: bool,
    saddle: bool,
) -> SplineTree {
    let very_low =
        build_mountain_ridge_spline(lerp_f32(mountain_factor, 0.6, 1.5), saddle);
    let low =
        build_mountain_ridge_spline(lerp_f32(mountain_factor, 0.6, 1.0), saddle);
    let mountains = build_mountain_ridge_spline(mountain_factor, saddle);

    let wide_plateau = ridge_spline(
        low_valley - 0.15,
        0.5 * mountain_factor,
        lerp_f32(0.5, 0.5, 0.5) * mountain_factor, // = 0.5 * mountainFactor
        0.5 * mountain_factor,
        0.6 * mountain_factor,
        0.5,
    );
    let narrow_plateau = ridge_spline(
        low_valley,
        plain * mountain_factor,
        hill * mountain_factor,
        0.5 * mountain_factor,
        0.6 * mountain_factor,
        0.5,
    );
    let plains = ridge_spline(low_valley, plain, plain, hill, tall_hill, 0.5);
    let plains_far_inland = ridge_spline(low_valley, plain, plain, hill, tall_hill, 0.5);
    let extreme_hills = SplineTree::Spline {
        axis: Axis::Ridges,
        nodes: vec![
            SplineNode { loc: -1.0, val: SplineTree::Const(low_valley as f64), der: 0.0 },
            SplineNode { loc: -0.4, val: plains.clone(), der: 0.0 },
            SplineNode {
                loc: 0.0,
                val: SplineTree::Const((tall_hill + 0.07) as f64),
                der: 0.0,
            },
        ],
    };
    let swamps = ridge_spline(-0.02, swamp, swamp, hill, tall_hill, 0.0);

    let mut nodes = vec![
        SplineNode { loc: -0.85, val: very_low, der: 0.0 },
        SplineNode { loc: -0.7, val: low, der: 0.0 },
        SplineNode { loc: -0.4, val: mountains, der: 0.0 },
        SplineNode { loc: -0.35, val: wide_plateau, der: 0.0 },
        SplineNode { loc: -0.1, val: narrow_plateau, der: 0.0 },
        SplineNode { loc: 0.2, val: plains, der: 0.0 },
    ];
    if include_extreme_hills {
        nodes.push(SplineNode { loc: 0.4, val: plains_far_inland.clone(), der: 0.0 });
        nodes.push(SplineNode { loc: 0.45, val: extreme_hills.clone(), der: 0.0 });
        nodes.push(SplineNode { loc: 0.55, val: extreme_hills, der: 0.0 });
        nodes.push(SplineNode { loc: 0.58, val: plains_far_inland, der: 0.0 });
    }
    nodes.push(SplineNode { loc: 0.7, val: swamps, der: 0.0 });

    SplineTree::Spline { axis: Axis::Erosion, nodes }
}

// --- 公开接口 -------------------------------------------------------------

// 对应 TerrainProvider.overworldOffset（非 amplified 版本）
// 输出加了 GLOBAL_OFFSET = -0.50375
pub fn overworld_offset(continents: f64, erosion: f64, ridges: f64) -> f64 {
    // -0.15F, 0.0F, 0.0F, 0.1F, 0.0F, -0.03F, false, false
    let beach =
        build_erosion_offset_spline(-0.15, 0.0, 0.0, 0.1, 0.0, -0.03, false, false);
    // -0.1F, 0.03F, 0.1F, 0.1F, 0.01F, -0.03F, false, false
    let low =
        build_erosion_offset_spline(-0.1, 0.03, 0.1, 0.1, 0.01, -0.03, false, false);
    // -0.1F, 0.03F, 0.1F, 0.7F, 0.01F, -0.03F, true, true
    let mid =
        build_erosion_offset_spline(-0.1, 0.03, 0.1, 0.7, 0.01, -0.03, true, true);
    // -0.05F, 0.03F, 0.1F, 1.0F, 0.01F, 0.01F, true, true
    let high =
        build_erosion_offset_spline(-0.05, 0.03, 0.1, 1.0, 0.01, 0.01, true, true);

    let cont_spline = SplineTree::Spline {
        axis: Axis::Continents,
        nodes: vec![
            SplineNode { loc: -1.1, val: SplineTree::Const(0.044), der: 0.0 },
            SplineNode { loc: -1.02, val: SplineTree::Const(-0.2222), der: 0.0 },
            SplineNode { loc: -0.51, val: SplineTree::Const(-0.2222), der: 0.0 },
            SplineNode { loc: -0.44, val: SplineTree::Const(-0.12), der: 0.0 },
            SplineNode { loc: -0.18, val: SplineTree::Const(-0.12), der: 0.0 },
            SplineNode { loc: -0.16, val: beach.clone(), der: 0.0 },
            SplineNode { loc: -0.15, val: beach, der: 0.0 },
            SplineNode { loc: -0.1, val: low, der: 0.0 },
            SplineNode { loc: 0.25, val: mid, der: 0.0 },
            SplineNode { loc: 1.0, val: high, der: 0.0 },
        ],
    };

    // GLOBAL_OFFSET = -0.50375
    cont_spline.eval(continents, erosion, ridges) - 0.503_75
}

// 对应 TerrainProvider.overworldFactor（非 amplified 版本）
// 输出乘数（约 1..10）越大地形起伏越强
pub fn overworld_factor(continents: f64, erosion: f64, ridges: f64) -> f64 {
    let cont_spline = SplineTree::Spline {
        axis: Axis::Continents,
        nodes: vec![
            SplineNode { loc: -0.19, val: SplineTree::Const(3.95), der: 0.0 },
            SplineNode {
                loc: -0.15,
                val: get_erosion_factor(6.25, true),
                der: 0.0,
            },
            SplineNode {
                loc: -0.1,
                val: get_erosion_factor(5.47, true),
                der: 0.0,
            },
            SplineNode {
                loc: 0.03,
                val: get_erosion_factor(5.08, true),
                der: 0.0,
            },
            SplineNode {
                loc: 0.06,
                val: get_erosion_factor(4.69, false),
                der: 0.0,
            },
        ],
    };
    cont_spline.eval(continents, erosion, ridges)
}

// 对应 TerrainProvider.peaksAndValleys（ridges 轴的折叠）
// 把 weirdness（ridge 噪声原值）转换成峰谷值
pub fn peaks_and_valleys_folded(weirdness: f64) -> f64 {
    peaks_and_valleys(weirdness as f32) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    // 深海区域（大陆性 = -0.6）的 offset 应该是负数
    #[test]
    fn deep_ocean_offset_is_negative() {
        let v = overworld_offset(-0.6, 0.0, 0.0);
        assert!(v < -0.1, "深海 offset={v}");
    }

    // 陆地区域（大陆性 0.5）offset 应高于深海
    #[test]
    fn inland_offset_is_above_ocean() {
        let inland = overworld_offset(0.5, -0.3, 0.3);
        let ocean = overworld_offset(-0.6, 0.0, 0.0);
        assert!(inland > ocean, "陆地 offset={inland} 应高于海洋 offset={ocean}");
        // 陆地 offset 至少 > -0.5（不能是深海底）
        assert!(inland > -0.3, "陆地 offset={inland} 过低");
    }

    // factor 应该在合理范围内
    #[test]
    fn factor_in_range() {
        let v = overworld_factor(0.1, 0.0, 0.0);
        assert!(v > 0.0 && v < 20.0, "factor={v}");
    }

    // peaks_and_valleys_folded 对称
    #[test]
    fn peaks_and_valleys_symmetry() {
        let a = peaks_and_valleys_folded(0.3);
        let b = peaks_and_valleys_folded(-0.3);
        assert!((a - b).abs() < 1e-6, "应该对称 {a} vs {b}");
    }

    // 在 continents=-1.1 时 offset 应等于 0.044 - 0.50375
    #[test]
    fn outer_ocean_offset_matches_constant() {
        let v = overworld_offset(-1.1, 0.0, 0.0);
        let expected = 0.044 - 0.503_75;
        assert!((v - expected).abs() < 1e-5, "v={v} expected={expected}");
    }
}
