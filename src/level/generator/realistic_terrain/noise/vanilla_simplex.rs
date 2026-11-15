//! 对应 `rtg/noise/VanillaBiomeNoise.php` 与 `VanillaTemperatureNoise.php`。
//!
//! 两者代码完全相同，只有种子不同：
//! - 生物群系颜色噪声：`Biome.GRASS_COLOR_NOISE`，seed = 2345
//! - 温度噪声：`Biome.TEMPERATURE_NOISE`，seed = 1234
//!
//! 都是 Minecraft 早期的单层 `NoiseGeneratorSimplex`。

use std::sync::OnceLock;

use crate::level::generator::realistic_terrain::math::JavaRandom;

const GRAD: [[f64; 2]; 12] = [
    [1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [-1.0, -1.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [1.0, 0.0],
    [-1.0, 0.0],
    [0.0, 1.0],
    [0.0, -1.0],
    [0.0, 1.0],
    [0.0, -1.0],
];

pub struct VanillaSimplex {
    p: [i32; 512],
}

/// Java 的 `(int)` 截断，负数时比 `floor` 小 1。原实现就是这个语义，照抄。
#[inline]
fn fast_floor(v: f64) -> i32 {
    if v > 0.0 { v as i32 } else { v as i32 - 1 }
}

#[inline]
fn corner(gradient: usize, x: f64, z: f64) -> f64 {
    let mut t = 0.5 - x * x - z * z;
    if t < 0.0 {
        return 0.0;
    }
    t *= t;
    t * t * (GRAD[gradient][0] * x + GRAD[gradient][1] * z)
}

impl VanillaSimplex {
    pub fn new(seed: i64) -> Self {
        let mut random = JavaRandom::new(seed);
        // 原实现会先取三个偏移量（xo/yo/zo），但 sample() 从未使用；
        // 这里保留取数动作，避免改变后续 nextInt 的随机序列。
        let _ = random.next_double();
        let _ = random.next_double();
        let _ = random.next_double();

        let mut p = [0i32; 512];
        for i in 0..256 {
            p[i] = i as i32;
        }
        for i in 0..256 {
            let j = random.next_int_bound(256 - i as i32) + i as i32;
            let v = p[i];
            p[i] = p[j as usize];
            p[j as usize] = v;
            p[i + 256] = p[i];
        }
        Self { p }
    }

    pub fn value(&self, x: f64, z: f64) -> f64 {
        let sqrt3 = 3.0_f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let g2 = (3.0 - sqrt3) / 6.0;

        let s = (x + z) * f2;
        let i = fast_floor(x + s);
        let j = fast_floor(z + s);
        let t = (i + j) as f64 * g2;
        let x0 = x - (i as f64 - t);
        let z0 = z - (j as f64 - t);

        let (i1, j1) = if x0 > z0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - i1 as f64 + g2;
        let z1 = z0 - j1 as f64 + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let z2 = z0 - 1.0 + 2.0 * g2;

        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;
        let g0 = (self.p[ii + self.p[jj] as usize] % 12) as usize;
        let g1 = (self.p[ii + i1 as usize + self.p[jj + j1 as usize] as usize] % 12) as usize;
        let g2i = (self.p[ii + 1 + self.p[jj + 1] as usize] % 12) as usize;

        70.0 * (corner(g0, x0, z0) + corner(g1, x1, z1) + corner(g2i, x2, z2))
    }
}

static BIOME_NOISE: OnceLock<VanillaSimplex> = OnceLock::new();
static TEMPERATURE_NOISE: OnceLock<VanillaSimplex> = OnceLock::new();

/// `Biome.GRASS_COLOR_NOISE`（seed 2345）
pub fn biome_noise(x: f64, z: f64) -> f64 {
    BIOME_NOISE
        .get_or_init(|| VanillaSimplex::new(2345))
        .value(x, z)
}

/// `Biome.TEMPERATURE_NOISE`（seed 1234）
pub fn temperature_noise(x: f64, z: f64) -> f64 {
    TEMPERATURE_NOISE
        .get_or_init(|| VanillaSimplex::new(1234))
        .value(x, z)
}
