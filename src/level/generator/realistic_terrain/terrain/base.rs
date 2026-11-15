//! 对应 `rtg/terrain/TerrainBase.php`（含 `PlateauUtil`）。
//!
//! 这里几乎每一行都套了 `F::f()`。它的作用是**把 double 立刻落成 float32 再升回 double**，
//! 模拟 Java 的 `float` 运算。漏掉任何一处，高度场就会整体漂移几格。
//!
//! 只移植了当前可达的函数。PHP 里还有 `terrainBryce` / `terrainCanyon` /
//! `terrainOceanCanyon` / `terrainForest` / `terrainVolcano`，
//! 它们没有被任何 `TerrainRegistry` 条目引用（死代码），等 carver 需要时再补。

use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::effects::VariableRuggednessEffect;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;

pub const MINIMUM_OCEAN_FLOOR: f64 = 15.010000228881836;
pub const MINIMUM_DUNE_HEIGHT: f64 = 21.0;

/// `noise2f`：入参和出参都按 float32 走。
#[inline]
pub fn noise2f(world: &RtgWorld, octave: usize, x: f64, y: f64) -> f64 {
    f(world
        .simplex_instance(octave)
        .noise2f(f(x) as f32, f(y) as f32) as f64)
}

#[inline]
pub fn noise3f(world: &RtgWorld, octave: usize, x: f64, y: f64, z: f64) -> f64 {
    f(world
        .simplex_instance(octave)
        .noise3f(f(x) as f32, f(y) as f32, f(z) as f32) as f64)
}

/// 把 simplex 的 `[-1,1]` 压成一条带平台的地形曲线。
pub fn blended_hill_height_n1(simplex: f64) -> f64 {
    let mut result = f(simplex + f(1.0));
    result = f(f(f(result * result) * result) + f(10.0));
    result = f(result.powf(0.33333333333333));
    result = f(result / 0.4663099944591522);
    result = f(result - 4.62021017074585);
    f(result)
}

pub fn blended_hill_height_n2(simplex: f64, turn_at: f64) -> f64 {
    let adjusted = f(1.0 - f(f(1.0 - simplex) / f(1.0 - turn_at)));
    f(blended_hill_height_n1(adjusted))
}

pub fn above(limited: f64, limit: f64) -> f64 {
    if limited > limit {
        return f(limited - limit);
    }
    f(0.0)
}

/// 保留符号的幂。
pub fn unsigned_power(number: f64, power: f64) -> f64 {
    if number > f(0.0) {
        return f(number.powf(power));
    }
    f(-1.0 * f((-number).powf(power)))
}

pub fn hills(x: f64, y: f64, hill_strength: f64, world: &RtgWorld) -> f64 {
    let mut m = noise2f(world, 0, f(x / 150.0), f(y / 150.0));
    m = f(blended_hill_height_n2(m, 0.20000000298023224));
    let mut sm = noise2f(world, 2, f(x / f(55.0)), f(y / f(55.0)));
    sm = f(blended_hill_height_n2(sm, 0.20000000298023224));
    sm = f(sm * f(sm * m));
    m = f(m + f(sm / 3.0));
    f(m * hill_strength)
}

pub fn ground_noise(x: f64, y: f64, amplitude: f64, world: &RtgWorld) -> f64 {
    let mut h = f(blended_hill_height_n2(
        noise2f(world, 0, f(x / 49.0), f(y / 49.0)),
        0.20000000298023224,
    ) * amplitude);
    h = f(h + f(f(blended_hill_height_n2(
        noise2f(world, 1, f(x / 23.0), f(y / 23.0)),
        0.20000000298023224,
    ) * amplitude)
        / 2.0));
    h = f(h + f(f(blended_hill_height_n2(
        noise2f(world, 2, f(x / 11.0), f(y / 11.0)),
        0.20000000298023224,
    ) * amplitude)
        / 4.0));
    f(h)
}

pub fn get_terrain_base_n0() -> f64 {
    f(68.0)
}

pub fn get_terrain_base_n1(river: f64) -> f64 {
    f(62.0 + f(6.0 * river))
}

pub fn mountain_cap(m: f64) -> f64 {
    let mut m = m;
    if m > f(160.0) {
        m = f(f(160.0) + f(f(m - f(160.0)) * 0.75));
        if m > f(180.0) {
            m = f(f(180.0) + f(f(m - 180.0) * 0.75));
        }
    }
    f(m)
}

/// 河道把地形压向水面，`river` 越小压得越狠。
pub fn riverized(height: f64, river: f64) -> f64 {
    if height < 62.45000076293945 {
        return f(height);
    }
    let adjustment = f(f(f(height - 62.45000076293945) / 10.0) + 0.6000000238418579);
    let river = f(bayesian_adjustment(river, adjustment));
    f(62.45000076293945 + f(f(height - 62.45000076293945) * river))
}

pub fn terrain_beach(x: f64, y: f64, world: &RtgWorld, river: f64, base_height: f64) -> f64 {
    f(riverized(
        f(base_height + ground_noise(x, y, 4.0, world)),
        river,
    ))
}

pub fn terrain_flat_lakes(x: f64, y: f64, world: &RtgWorld, river: f64, base_height: f64) -> f64 {
    let rugged_noise = noise2f(
        world,
        1,
        f(x / VariableRuggednessEffect::STANDARD_RUGGEDNESS_WAVELENGTH),
        f(y / VariableRuggednessEffect::STANDARD_RUGGEDNESS_WAVELENGTH),
    );
    let rugged_noise = f(blended_hill_height_n1(rugged_noise));
    let h = f(ground_noise(x, y, f(2.0 * f(rugged_noise + 1.0)), world));
    f(riverized(f(base_height + h), river))
}

pub fn terrain_grassland_flats(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    m_pitch: f64,
    base_height: f64,
) -> f64 {
    let mut h = f(noise2f(world, 0, f(x / 100.0), f(y / 100.0)) * f(7.0));
    h = f(h + f(noise2f(world, 0, f(x / 20.0), f(y / 20.0)) * f(2.0)));
    let mut m = f(f(noise2f(world, 0, f(x / 180.0), f(y / 180.0)) * 35.0) * river);
    m = f(m * f(f(m) / m_pitch));
    let mut sm = f(blended_hill_height_n1(noise2f(world, 0, f(x / 30.0), f(y / 30.0))) * 8.0);
    let ratio = f(f(m) / 20.0);
    sm = f(sm * if ratio > 3.75 { 3.75 } else { ratio });
    m = f(m + sm);
    f(riverized(f(f(base_height + h) + m), river))
}

pub fn terrain_grassland_hills(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    v_width: f64,
    v_height: f64,
    h_width: f64,
    h_height: f64,
    b_height: f64,
) -> f64 {
    let mut h = noise2f(world, 0, f(x / v_width), f(y / v_width));
    h = f(blended_hill_height_n2(h, 0.30000001192092896));
    let mut m = noise2f(world, 1, f(x / h_width), f(y / h_width));
    m = f(blended_hill_height_n2(m, 0.30000001192092896) * h);
    m = f(m * m);
    h = f(h * f(v_height * river));
    m = f(m * f(h_height * river));
    h = f(h + ground_noise(x, y, 4.0, world));
    f(riverized(f(b_height + h), river) + m)
}

pub fn terrain_grassland_mountains(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    h_factor: f64,
    m_factor: f64,
    base_height: f64,
) -> f64 {
    let mut h = f(noise2f(world, 0, f(x / 100.0), f(y / 100.0)) * h_factor);
    h = f(h + f(noise2f(world, 0, f(x / 20.0), f(y / 20.0)) * f(2.0)));
    let mut m = f(f(noise2f(world, 0, f(x / 230.0), f(y / 230.0)) * m_factor) * river);
    m = f(m * f(f(m) / 35.0));
    m = f(if m > 70.0 {
        f(70.0 + f(f(m - 70.0) / 2.5))
    } else {
        m
    });
    let c = f(noise3f(world, 4, f(x / 30.0), f(y / 30.0), 1.0) * f(m * 0.30000001192092896));
    let mut sm = f(f(noise2f(world, 0, f(x / 30.0), f(y / 30.0)) * 8.0)
        + noise2f(world, 0, f(x / 8.0), f(y / 8.0)));
    let ratio = f(f(m) / 20.0);
    sm = f(sm * if ratio > 2.5 { 2.5 } else { ratio });
    m = f(m + sm);
    m = f(m + c);
    f(riverized(f(f(base_height + h) + m), river))
}

#[allow(clippy::too_many_arguments)]
pub fn terrain_highland(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    start: f64,
    width: f64,
    height: f64,
    base_adjust: f64,
) -> f64 {
    let mut h = f(f(noise2f(world, 0, f(x / width), f(y / width)) * height) * river);
    h = f(if h < start {
        f(start + f(f(h - start) / 4.5))
    } else {
        h
    });
    if h < 0.0 {
        h = f(0.0);
    }
    if h > 0.0 {
        let st = f(if f(h * 1.5) > 15.0 { 15.0 } else { f(h * 1.5) });
        h = f(h + f(noise3f(world, 4, f(x / 70.0), f(y / 70.0), 1.0) * st));
        h = f(h * river);
    }
    h = f(h + f(blended_hill_height_n2(noise2f(world, 0, f(x / 20.0), f(y / 20.0)), 0.0) * 4.0));
    h = f(h + f(blended_hill_height_n2(noise2f(world, 0, f(x / 12.0), f(y / 12.0)), 0.0) * 2.0));
    h = f(h + f(blended_hill_height_n2(noise2f(world, 0, f(x / 5.0), f(y / 5.0)), 0.0) * 1.0));
    if h < f(0.0) {
        h = f(f(h) / 2.0);
    }
    if h < f(-3.0) {
        h = f(f(f(h + 3.0) / 2.0) - 3.0);
    }
    f(get_terrain_base_n1(river) + f(f(h + base_adjust) * river))
}

pub fn terrain_lonely_mountain(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    strength: f64,
    width: f64,
    terrain_height: f64,
) -> f64 {
    let mut h =
        f(blended_hill_height_n2(noise2f(world, 0, f(x / 20.0), f(y / 20.0)), f(0.0)) * f(3.0));
    h = f(h + f(
        blended_hill_height_n2(noise2f(world, 0, f(x / 7.0), f(y / 7.0)), f(0.0))
            * 1.2999999523162842,
    ));
    let mut m = f(f(noise2f(world, 0, f(x / width), f(y / width)) * strength) * river);
    m = f(m * f(f(m) / 35.0));
    m = f(if m > 70.0 {
        f(70.0 + f(f(m - 70.0) / 2.5))
    } else {
        m
    });
    let mut st = f(m * 0.699999988079071);
    st = f(if st > 20.0 { 20.0 } else { st });
    let c = f(noise3f(world, 4, f(x / 30.0), f(y / 30.0), 1.0) * f(5.0 + st));
    let mut sm = f(f(noise2f(world, 0, f(x / 30.0), f(y / 30.0)) * 8.0)
        + noise2f(world, 0, f(x / 8.0), f(y / 8.0)));
    let ratio = f(f(m + 10.0) / 20.0);
    sm = f(sm * if ratio > 2.5 { 2.5 } else { ratio });
    m = f(m + sm);
    m = f(m + c);
    if m > f(90.0) {
        m = f(90.0 + f(f(m - 90.0) * 0.75));
        if m > f(110.0) {
            m = f(110.0 + f(f(m - 110.0) * 0.75));
        }
    }
    f(riverized(f(f(terrain_height + h) + m), river))
}

pub fn terrain_marsh(x: f64, y: f64, world: &RtgWorld, base_height: f64, river: f64) -> f64 {
    let mut h = f(noise2f(world, 0, f(x / 130.0), f(y / 130.0)) * 20.0);
    h = f(h + f(noise2f(world, 0, f(x / 12.0), f(y / 12.0)) * 2.0));
    h = f(h + f(noise2f(world, 0, f(x / 18.0), f(y / 18.0)) * 4.0));
    h = f(if h < 8.0 { 0.0 } else { f(h - 8.0) });
    if h == 0.0 {
        h =
            f(h + f(noise2f(world, 0, f(x / 20.0), f(y / 20.0))
                + noise2f(world, 0, f(x / 5.0), f(y / 5.0))));
        h = f(h * 2.0);
    }
    f(riverized(f(base_height + h), river))
}

pub fn terrain_ocean(x: f64, y: f64, world: &RtgWorld, river: f64, average_floor: f64) -> f64 {
    let mut h = f(f(noise2f(world, 0, f(x / 300.0), f(y / 300.0)) * 8.0) * river);
    h = f(h + f(noise2f(world, 0, f(x / 50.0), f(y / 50.0)) * 2.0));
    h = f(h + f(noise2f(world, 0, f(x / 15.0), f(y / 15.0)) * 1.0));
    let flo_noise = f(average_floor + h);
    f(if flo_noise < MINIMUM_OCEAN_FLOOR {
        MINIMUM_OCEAN_FLOOR
    } else {
        flo_noise
    })
}

#[allow(clippy::too_many_arguments)]
pub fn terrain_plains(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    st_pitch: f64,
    st_factor: f64,
    h_pitch: f64,
    h_divisor: f64,
    base_height: f64,
) -> f64 {
    let mut st = f(f(
        f(noise2f(world, 0, f(x / st_pitch), f(y / st_pitch)) + 0.3799999952316284) * st_factor,
    ) * river);
    st = f(if st < 0.20000000298023224 {
        0.20000000298023224
    } else {
        st
    });
    let mut h = f(f(noise2f(world, 0, f(x / h_pitch), f(y / h_pitch)) * st) * 2.0);
    h = f(if h > 0.0 { -h } else { h });
    h = f(h + st);
    h = f(h * f(f(h) / h_divisor));
    h = f(h + st);
    f(riverized(f(base_height + h), river))
}

#[allow(clippy::too_many_arguments)]
pub fn terrain_polar(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    st_pitch: f64,
    st_factor: f64,
    h_pitch: f64,
    h_divisor: f64,
    base_height: f64,
) -> f64 {
    let mut st = f(f(
        f(noise2f(world, 0, f(x / st_pitch), f(y / st_pitch)) + 0.3799999952316284) * st_factor,
    ) * river);
    st = f(if st < 0.10000000149011612 {
        0.10000000149011612
    } else {
        st
    });
    let mut h = f(f(noise2f(world, 0, f(x / h_pitch), f(y / h_pitch)) * st) * 2.0);
    h = f(if h > 0.0 { -h } else { h });
    h = f(h + st);
    h = f(h * f(f(h) / h_divisor));
    h = f(h + st);
    f(riverized(f(base_height + h), river))
}

#[allow(clippy::too_many_arguments)]
pub fn terrain_plateau(
    x: f64,
    y: f64,
    world: &RtgWorld,
    river: f64,
    height: &[f64],
    border: f64,
    strength: f64,
    height_length: usize,
    selector_wavelength: f64,
    is_m: bool,
) -> f64 {
    let river = f(if river > 1.0 { 1.0 } else { river });
    let border2 = f(f(border * f(4.0)) - 2.5);
    let border2 = f(if border2 > 1.0 {
        1.0
    } else if border2 < 0.0 {
        0.0
    } else {
        border2
    });

    let mut b = f(noise2f(world, 0, f(x / 40.0), f(y / 40.0)) * 1.5);
    let mut sn = f(f(noise2f(
        world,
        0,
        f(x / selector_wavelength),
        f(y / selector_wavelength),
    ) * 0.5)
        + 0.5);
    sn = f(sn * border2);
    sn = f(sn * river);
    sn = f(sn
        + f(
            f(noise2f(world, 0, f(x / 4.0), f(y / 4.0)) * 0.009999999776482582)
                + 0.009999999776482582,
        ));
    sn = f(sn
        + f(
            f(noise2f(world, 0, f(x / 2.0), f(y / 2.0)) * 0.009999999776482582)
                + 0.009999999776482582,
        ));

    let mut i = 0usize;
    while i < height_length {
        let lower = height[i];
        let upper = height[i + 1];

        let mut n = f(f(sn - upper) / f(f(1.0) - upper));
        n = f(n * strength);
        n = f(if n < 0.0 {
            0.0
        } else if n > 1.0 {
            1.0
        } else {
            n
        });

        let mut hn = f(f(lower * 0.5) * f(f(sn * 2.0) - 0.4000000059604645));
        hn = f(if hn < f(0.0) { 0.0 } else { hn });

        let mut step_up = f(0.0);
        if sn > upper {
            step_up = f(step_up + f(lower * n));
            if is_m {
                step_up = f(step_up + f(f(noise2f(world, 0, f(x / 20.0), f(y / 20.0)) * 3.0) * n));
                step_up = f(step_up + f(f(noise2f(world, 0, f(x / 12.0), f(y / 12.0)) * 2.0) * n));
                step_up = f(step_up + f(f(noise2f(world, 0, f(x / 5.0), f(y / 5.0)) * 1.0) * n));
            }
        }

        if i == 0 && step_up < hn {
            b = f(b + hn);
        }
        step_up = f(if step_up < f(0.0) { 0.0 } else { step_up });
        b = f(b + step_up);

        i += 2;
    }

    if is_m {
        b = f(b + f(noise2f(world, 0, f(x / f(12.0)), f(y / f(12.0))) * sn));
    }
    b = f(b / border);
    f(riverized(get_terrain_base_n0(), river) + b)
}

/// 河网强度：先按两级 simplex 抖动坐标，再用 cellular 的边界场取比例。
///
/// 返回值恒为 `<= 0`；调用方加 1 之后才是 0..1 的河流权重。
pub fn get_river_strength(world_x: i32, world_z: i32, world: &RtgWorld) -> f64 {
    let mut p_x = world_x as f64;
    let mut p_z = world_z as f64;

    // `F::div(F::f($worldX), 240.0)`：只截断入参，商本身留在 double 精度
    let delta = world.simplex_instance(1).multi_eval2d(
        f(world_x as f64) / 240.0,
        f(world_z as f64) / 240.0,
        false,
    );
    p_x += delta[0] * world.get_river_large_bend_size();
    p_z += delta[1] * world.get_river_large_bend_size();

    let delta = world.simplex_instance(2).multi_eval2d(
        f(world_x as f64) / 80.0,
        f(world_z as f64) / 80.0,
        false,
    );
    p_x += delta[0] * world.get_river_small_bend_size();
    p_z += delta[1] * world.get_river_small_bend_size();

    p_x /= world.get_river_separation();
    p_z /= world.get_river_separation();

    let river_factor = world.cellular_eval(0, p_x, p_z).interior_value();
    let river_factor = bayesian_adjustment(f(river_factor), 0.5);
    if river_factor > world.get_river_valley_level() {
        return f(0.0);
    }
    f(river_factor / world.get_river_valley_level() - 1.0)
}

/// 河岸断崖：取四邻高差里“同一方向两侧都陡”的那一个。
pub fn calc_cliff(x: usize, z: usize, noise: &[f64], river: f64) -> f64 {
    let mut cliff = f(0.0);
    let here = noise[x * 16 + z];

    if here < 64.5 && here > 61.5 {
        if f(river
            + crate::level::generator::realistic_terrain::terrain::world::ACTUAL_RIVER_PROPORTION)
            > 0.9700000286102295
        {
            let x_up = f(if x < 15 {
                f((here - noise[(x + 1) * 16 + z]).abs())
            } else {
                0.0
            });
            let x_down = f(if x > 0 {
                f((here - noise[(x - 1) * 16 + z]).abs())
            } else {
                0.0
            });
            let z_up = f(if z < 15 {
                f((here - noise[x * 16 + z + 1]).abs())
            } else {
                0.0
            });
            let z_down = f(if z > 0 {
                f((here - noise[x * 16 + z - 1]).abs())
            } else {
                0.0
            });

            let x_cliff = f(x_up.min(x_down));
            let z_cliff = f(z_up.min(z_down));
            return f(x_cliff.max(z_cliff));
        }
    }

    if x > 0 {
        cliff = f(cliff.max(f((here - noise[(x - 1) * 16 + z]).abs())));
    }
    if z > 0 {
        cliff = f(cliff.max(f((here - noise[x * 16 + z - 1]).abs())));
    }
    if x < 15 {
        cliff = f(cliff.max(f((here - noise[(x + 1) * 16 + z]).abs())));
    }
    if z < 15 {
        cliff = f(cliff.max(f((here - noise[x * 16 + z + 1]).abs())));
    }
    f(cliff)
}

/// 贝叶斯式比例混合：把 `probability` 按 `multiplier` 推向 0 或 1。
pub fn bayesian_adjustment(probability: f64, multiplier: f64) -> f64 {
    if probability >= f(1.0) {
        return f(probability);
    }
    if probability <= f(0.0) {
        return f(probability);
    }
    let new_confidence = f(f(probability * multiplier) / f(1.0 - probability));
    f(new_confidence / f(1.0 + new_confidence))
}
