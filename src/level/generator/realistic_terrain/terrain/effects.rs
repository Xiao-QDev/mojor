//! 对应 `rtg/terrain/*Effect.php`（`HeightEffect` 家族）。
//!
//! PHP 用可变字段 + 构造函数重载来配置，Rust 改成结构体字面量 / 构造器参数。
//! 所有 `added()` 的 `x,y` 都已经是 `F::f()` 过的 f32 值。
//!
//! `SpikeEverywhereEffect` / `MountainsWithPassesEffect` 里的
//! `if ($noise > 1.01) throw` 是原作者的防御性断言，这里保留成 debug 断言。

use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::base;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;

pub trait HeightEffect: Send + Sync {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64;
}

/// 两个效果相加；对应 PHP 的 `HeightEffect::plus()`。
pub struct SummedHeightEffect {
    pub one: Box<dyn HeightEffect>,
    pub two: Box<dyn HeightEffect>,
}

impl HeightEffect for SummedHeightEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        f(self.one.added(world, x, y) + self.two.added(world, x, y))
    }
}

/// 常数列。`GroundEffect` / `RaiseEffect` 都属于这一类。
pub struct RaiseEffect {
    pub height: f64,
}

impl RaiseEffect {
    pub fn new(height: f64) -> Self {
        Self { height: f(height) }
    }
}

impl HeightEffect for RaiseEffect {
    fn added(&self, _world: &RtgWorld, _x: f64, _y: f64) -> f64 {
        f(self.height)
    }
}

/// 三层地面噪声，波长 49/23/11，振幅递减。
pub struct GroundEffect {
    pub amplitude: f64,
}

impl GroundEffect {
    pub fn new(amplitude: f64) -> Self {
        Self {
            amplitude: f(amplitude),
        }
    }
}

impl HeightEffect for GroundEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        f(base::ground_noise(x, y, self.amplitude, world))
    }
}

/// 单层 simplex，按波长缩放。
pub struct HeightVariation {
    pub height: f64,
    pub wavelength: f64,
    pub octave: usize,
}

impl HeightEffect for HeightVariation {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        f(
            base::noise2f(world, self.octave, x / self.wavelength, y / self.wavelength)
                * self.height,
        )
    }
}

/// 大丘陵 + 尖刺两层。
pub struct BumpyHillsEffect {
    pub hill_height: f64,
    pub hill_wavelength: f64,
    pub spike_height: f64,
    pub spike_wavelength: f64,
    pub hill_octave: usize,
    pub spike_octave: usize,
}

impl HeightEffect for BumpyHillsEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let mut noise = base::noise2f(
            world,
            self.hill_octave,
            x / self.hill_wavelength,
            y / self.hill_wavelength,
        );
        noise = base::blended_hill_height_n1(noise);
        let mut spike_noise = base::noise2f(
            world,
            self.spike_octave,
            x / self.spike_wavelength,
            y / self.spike_wavelength,
        );
        spike_noise = base::blended_hill_height_n1(f(spike_noise * noise));
        f(f(noise * self.hill_height) + f(spike_noise * self.spike_height))
    }
}

/// 只在高出阈值的区域叠加。
pub struct HillockEffect {
    pub height: f64,
    pub wavelength: f64,
    pub minimum_simplex: f64,
    pub octave: usize,
}

impl HeightEffect for HillockEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let mut noise = base::noise2f(world, self.octave, x / self.wavelength, y / self.wavelength);
        if noise < self.minimum_simplex {
            noise = f(0.0);
        } else {
            noise = f(f(noise - self.minimum_simplex) / f(1.0 - self.minimum_simplex));
        }
        f(noise * self.height)
    }
}

/// 用 simplex 导数做坐标抖动，打破噪声网格的方向性。
pub struct JitterEffect {
    pub amplitude: f64,
    pub wavelength: f64,
    pub jittered: Box<dyn HeightEffect>,
}

impl JitterEffect {
    pub fn new(amplitude: f64, wavelength: f64, jittered: Box<dyn HeightEffect>) -> Self {
        Self {
            amplitude: f(amplitude),
            wavelength: f(wavelength),
            jittered,
        }
    }
}

impl HeightEffect for JitterEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let delta = world.simplex_instance(1).multi_eval2d(
            f(x / self.wavelength),
            f(y / self.wavelength),
            false,
        );
        let px =
            crate::level::generator::realistic_terrain::math::round(x + delta[0] * self.amplitude);
        let py =
            crate::level::generator::realistic_terrain::math::round(y + delta[1] * self.amplitude);
        f(self.jittered.added(world, f(px as f64), f(py as f64)))
    }
}

/// 全域尖刺：`blendedHillHeightN2` 后取 `unsignedPower`，再乘子效果。
pub struct SpikeEverywhereEffect {
    pub wavelength: f64,
    pub minimum_simplex: f64,
    pub octave: usize,
    pub power: f64,
    pub spiked: Box<dyn HeightEffect>,
}

impl HeightEffect for SpikeEverywhereEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let mut noise = base::noise2f(world, self.octave, x / self.wavelength, y / self.wavelength);
        noise = f(noise.abs());
        noise = base::blended_hill_height_n2(noise, self.minimum_simplex);
        noise = base::unsigned_power(noise, self.power);
        f(noise * self.spiked.added(world, x, y))
    }
}

/// 平滑地形与崎岖地形之间的插值，按 `startTransition` 起飞。
pub struct VariableRuggednessEffect {
    pub smooth_terrain: Box<dyn HeightEffect>,
    pub rugged_terrain: Box<dyn HeightEffect>,
    pub start_transition: f64,
    pub transition_width: f64,
    pub octave: usize,
    pub wavelength: f64,
}

impl VariableRuggednessEffect {
    pub const STANDARD_RUGGEDNESS_WAVELENGTH: f64 = 200.0;
    pub const STANDARD_RUGGEDNESS_OCTAVE: usize = 1;
}

impl HeightEffect for VariableRuggednessEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let choice = base::noise2f(world, self.octave, x / self.wavelength, y / self.wavelength);
        if choice <= self.start_transition {
            return f(self.smooth_terrain.added(world, x, y));
        }
        if choice >= f(self.start_transition + self.transition_width) {
            return f(self.rugged_terrain.added(world, x, y));
        }
        let smooth = f(self.smooth_terrain.added(world, x, y));
        let rugged = f(self.rugged_terrain.added(world, x, y));
        let top = f(self.start_transition + self.transition_width);
        let towards_rugged = f(f(choice - self.start_transition) * rugged);
        let towards_smooth = f(f(top - choice) * smooth);
        let blended = f(towards_rugged + towards_smooth);
        f(blended / self.transition_width)
    }
}

/// 环状山脉，带垭口。
pub struct MountainsWithPassesEffect {
    pub mountain_height: f64,
    pub mountain_wavelength: f64,
    pub spike_height: f64,
    pub spike_wavelength: f64,
    pub hill_octave: usize,
    pub spike_octave: usize,
    pub adjusted_bottom: f64,
}

impl MountainsWithPassesEffect {
    pub fn new(
        mountain_height: f64,
        mountain_wavelength: f64,
        spike_height: f64,
        spike_wavelength: f64,
    ) -> Self {
        Self {
            mountain_height: f(mountain_height),
            mountain_wavelength: f(mountain_wavelength),
            spike_height: f(spike_height),
            spike_wavelength: f(spike_wavelength),
            hill_octave: 0,
            spike_octave: 2,
            adjusted_bottom: f(base::blended_hill_height_n2(f(0.0), 0.20000000298023224)),
        }
    }
}

impl HeightEffect for MountainsWithPassesEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let mut noise = base::noise2f(
            world,
            self.hill_octave,
            x / self.mountain_wavelength,
            y / self.mountain_wavelength,
        );
        noise = f(noise.abs());
        noise = base::blended_hill_height_n2(noise, 0.20000000298023224);
        noise = f(1.0 - f(f(1.0 - noise) / f(1.0 - self.adjusted_bottom)));

        let mut spike_noise = base::blended_hill_height_n2(noise, 0.10000000149011612);
        spike_noise = f(spike_noise * spike_noise);
        spike_noise = base::blended_hill_height_n1(f(spike_noise * noise));

        debug_assert!(noise <= 1.01, "RTG terrain invariant");
        debug_assert!(spike_noise <= 1.01, "RTG terrain invariant");

        f(f(noise * self.mountain_height) + f(spike_noise * self.spike_height))
    }
}

/// Voronoi 单元的边界值，用来切出山脊。
pub struct VoronoiBorderEffect {
    pub point_wavelength: f64,
}

impl HeightEffect for VoronoiBorderEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let points = world.cellular_eval(
            1,
            f(x / self.point_wavelength),
            f(y / self.point_wavelength),
        );
        let raise = f(points.interior_value());
        f(1.0 - raise)
    }
}

/// Voronoi 单元内部值，用来做台地。
pub struct VoronoiPlateauEffect {
    pub point_wavelength: f64,
    pub adjustment_radius: f64,
}

impl VoronoiPlateauEffect {
    pub fn new(point_wavelength: f64) -> Self {
        Self {
            point_wavelength: f(point_wavelength),
            adjustment_radius: 3.0,
        }
    }
}

impl HeightEffect for VoronoiPlateauEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let evaluate_at = [f(x / self.point_wavelength), f(y / self.point_wavelength)];
        let points = world.cellular_eval(1, evaluate_at[0], evaluate_at[1]);
        let mut raise = f(points.interior_value());
        let adjust_at = points.to_length(evaluate_at[0], evaluate_at[1], self.adjustment_radius);
        let multiplier = f(1.2999999523162842);
        let no_zeros = f(0.10000000149011612);

        let adjustment = f(f(world
            .cellular_eval(2, adjust_at[0], adjust_at[1])
            .interior_value()
            * multiplier)
            + no_zeros);
        let re_adjustment = f(f(world
            .cellular_eval(3, adjust_at[0], adjust_at[1])
            .interior_value()
            * multiplier)
            + no_zeros);

        let adjustment = f(base::bayesian_adjustment(adjustment, re_adjustment));
        raise = f(base::bayesian_adjustment(raise, adjustment));
        f(raise)
    }
}

/// Voronoi 单元边界做盆地。
pub struct VoronoiBasinEffect {
    pub point_wavelength: f64,
    pub adjustment_radius: f64,
}

impl VoronoiBasinEffect {
    pub fn new(point_wavelength: f64) -> Self {
        Self {
            point_wavelength: f(point_wavelength),
            adjustment_radius: 1.0,
        }
    }
}

impl HeightEffect for VoronoiBasinEffect {
    fn added(&self, world: &RtgWorld, x: f64, y: f64) -> f64 {
        let evaluate_at = [f(x / self.point_wavelength), f(y / self.point_wavelength)];
        let points = world.cellular_eval(1, evaluate_at[0], evaluate_at[1]);
        let mut raise = f(points.border_value());
        let adjust_at = points.to_length(evaluate_at[0], evaluate_at[1], self.adjustment_radius);
        let no_zeros = f(0.25);

        let adjustment = f(world
            .cellular_eval(2, adjust_at[0], adjust_at[1])
            .interior_value()
            + no_zeros);
        let re_adjustment = f(world
            .cellular_eval(3, adjust_at[0], adjust_at[1])
            .interior_value()
            + no_zeros);

        let adjustment = f(base::bayesian_adjustment(adjustment, re_adjustment));
        raise = f(base::bayesian_adjustment(
            raise,
            f(1.0 / f(adjustment * adjustment)),
        ));
        f(raise)
    }
}
