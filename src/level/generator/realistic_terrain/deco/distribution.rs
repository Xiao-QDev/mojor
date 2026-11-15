//! 对应 `rtg/deco/Distribution.php`。
//!
//! 一个"噪声 → 数量"的映射：把 OpenSimplex 的 -1..1 值除以 `noise_divisor`
//! 采样后再乘 `noise_factor` 加 `noise_addend`。RTG 用它控制树、花的疏密变化，
//! 所以同一片森林的密度会随位置缓慢起伏，而不是均匀撒点。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::noise::open_simplex::OpenSimplexNoise;
use crate::level::generator::realistic_terrain::tree::f;

#[derive(Clone, Copy, Debug)]
pub struct Distribution {
    pub noise_divisor: f64,
    pub noise_factor: f64,
    pub noise_addend: f64,
}

impl Distribution {
    pub fn new(noise_divisor: f64, noise_factor: f64, noise_addend: f64) -> Self {
        Self {
            noise_divisor: f(noise_divisor),
            noise_factor: f(noise_factor),
            noise_addend: f(noise_addend),
        }
    }

    /// 对应 `getValue`：在 `offset_pos` 处取一个密度系数。
    pub fn value(&self, offset_pos: Pos, noise: &OpenSimplexNoise) -> f64 {
        let nx = f(offset_pos.x as f64) / self.noise_divisor;
        let nz = f(offset_pos.z as f64) / self.noise_divisor;
        let sample = noise.noise2f(nx as f32, nz as f32) as f64;
        f(f(sample * self.noise_factor) + self.noise_addend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_stays_within_addend_plus_factor() {
        let noise = OpenSimplexNoise::new(12345);
        let distribution = Distribution::new(100.0, 5.0, 0.8);
        for x in -200..200 {
            let value = distribution.value(Pos::new(x * 16 + 8, 0, x * 7), &noise);
            assert!(
                (-6.0..=7.0).contains(&value),
                "({x}) 密度 {value} 超出预期范围"
            );
        }
    }

    #[test]
    fn larger_divisor_makes_slower_variation() {
        let noise = OpenSimplexNoise::new(99);
        let fine = Distribution::new(10.0, 1.0, 0.0);
        let coarse = Distribution::new(1000.0, 1.0, 0.0);
        let mut fine_change = 0.0;
        let mut coarse_change = 0.0;
        let mut previous = (
            fine.value(Pos::new(0, 0, 0), &noise),
            coarse.value(Pos::new(0, 0, 0), &noise),
        );
        for x in 1..50 {
            let pos = Pos::new(x * 16, 0, 0);
            let current = (fine.value(pos, &noise), coarse.value(pos, &noise));
            fine_change += (current.0 - previous.0).abs();
            coarse_change += (current.1 - previous.1).abs();
            previous = current;
        }
        assert!(
            fine_change > coarse_change,
            "小除数应该变化更快：{fine_change} vs {coarse_change}"
        );
    }
}
