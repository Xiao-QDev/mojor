// 对齐 Nukkit 的 noise 包：Perlin 与 Simplex 共用一份置换表
// 噪声值是 3D 的 2D 采样只是 z 固定为 0 这也是 Nukkit 地形的原始行为
use crate::level::generator::random::NukkitRandom;

const GRAD3: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

const SQRT_3: f64 = 1.732_050_807_568_877_2;
const F2: f64 = 0.5 * (SQRT_3 - 1.0);
const G2: f64 = (3.0 - SQRT_3) / 6.0;
const G22: f64 = G2 * 2.0 - 1.0;
const F3: f64 = 1.0 / 3.0;
const G3: f64 = 1.0 / 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoiseKind {
    Perlin,
    Simplex,
}

#[derive(Clone)]
pub struct Noise {
    kind: NoiseKind,
    perm: [i32; 512],
    offset_x: f64,
    offset_y: f64,
    offset_z: f64,
    octaves: f64,
    persistence: f64,
    expansion: f64,
}

impl Noise {
    // 构造会按 Nukkit 的顺序消耗随机数 顺序错了整张地形就变了
    pub fn new(
        kind: NoiseKind,
        random: &mut NukkitRandom,
        octaves: f64,
        persistence: f64,
        expansion: f64,
    ) -> Self {
        let offset_x = random.next_float() as f64 * 256.0;
        let offset_y = random.next_float() as f64 * 256.0;
        let offset_z = random.next_float() as f64 * 256.0;

        let mut perm = [0i32; 512];
        for slot in perm.iter_mut().take(256) {
            *slot = random.next_bounded_int(256);
        }
        for i in 0..256 {
            let pos = (random.next_bounded_int(256 - i as i32) + i as i32) as usize;
            perm.swap(i, pos);
            perm[i + 256] = perm[i];
        }

        // Simplex 多消耗一个 offsetW 虽然没用到 但序列要一致
        if kind == NoiseKind::Simplex {
            let _offset_w = random.next_double() * 256.0;
        }

        Self {
            kind,
            perm,
            offset_x,
            offset_y,
            offset_z,
            octaves,
            persistence,
            expansion,
        }
    }

    pub fn noise2d(&self, x: f64, z: f64, normalized: bool) -> f64 {
        self.accumulate(x, 0.0, z, normalized, false)
    }

    pub fn noise3d(&self, x: f64, y: f64, z: f64, normalized: bool) -> f64 {
        self.accumulate(x, y, z, normalized, true)
    }

    fn accumulate(&self, x: f64, y: f64, z: f64, normalized: bool, three_d: bool) -> f64 {
        let mut result = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut max = 0.0;

        let x = x * self.expansion;
        let y = y * self.expansion;
        let z = z * self.expansion;

        // Nukkit 用 int 计数器与 double 上界比较 4.1 会跑 5 层
        let mut octave = 0i32;
        while (octave as f64) < self.octaves {
            let sample = if three_d {
                self.sample3d(x * frequency, y * frequency, z * frequency)
            } else {
                self.sample2d(x * frequency, z * frequency)
            };
            result += sample * amplitude;
            max += amplitude;
            frequency *= 2.0;
            amplitude *= self.persistence;
            octave += 1;
        }

        if normalized { result / max } else { result }
    }

    fn sample2d(&self, x: f64, z: f64) -> f64 {
        match self.kind {
            NoiseKind::Perlin => self.perlin3d(x, z, 0.0),
            NoiseKind::Simplex => self.simplex2d(x, z),
        }
    }

    fn sample3d(&self, x: f64, y: f64, z: f64) -> f64 {
        match self.kind {
            NoiseKind::Perlin => self.perlin3d(x, y, z),
            NoiseKind::Simplex => self.simplex3d(x, y, z),
        }
    }

    fn perlin3d(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.offset_x;
        let y = y + self.offset_y;
        let z = z + self.offset_z;

        // Nukkit 用截断取整 不是 floor 负坐标下会有偏差 这里保持一致
        let floor_x = x as i32;
        let floor_y = y as i32;
        let floor_z = z as i32;

        let xi = floor_x & 0xff;
        let yi = floor_y & 0xff;
        let zi = floor_z & 0xff;

        let x = x - floor_x as f64;
        let y = y - floor_y as f64;
        let z = z - floor_z as f64;

        let fx = x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
        let fy = y * y * y * (y * (y * 6.0 - 15.0) + 10.0);
        let fz = z * z * z * (z * (z * 6.0 - 15.0) + 10.0);

        let a = self.perm[xi as usize] + yi;
        let b = self.perm[xi as usize + 1] + yi;

        let aa = self.perm[a as usize] + zi;
        let ab = self.perm[a as usize + 1] + zi;
        let ba = self.perm[b as usize] + zi;
        let bb = self.perm[b as usize + 1] + zi;

        let aa1 = grad(self.perm[aa as usize], x, y, z);
        let ba1 = grad(self.perm[ba as usize], x - 1.0, y, z);
        let ab1 = grad(self.perm[ab as usize], x, y - 1.0, z);
        let bb1 = grad(self.perm[bb as usize], x - 1.0, y - 1.0, z);
        let aa2 = grad(self.perm[aa as usize + 1], x, y, z - 1.0);
        let ba2 = grad(self.perm[ba as usize + 1], x - 1.0, y, z - 1.0);
        let ab2 = grad(self.perm[ab as usize + 1], x, y - 1.0, z - 1.0);
        let bb2 = grad(self.perm[bb as usize + 1], x - 1.0, y - 1.0, z - 1.0);

        let x_lerp_11 = aa1 + fx * (ba1 - aa1);
        let z_lerp_1 = x_lerp_11 + fy * (ab1 + fx * (bb1 - ab1) - x_lerp_11);
        let x_lerp_21 = aa2 + fx * (ba2 - aa2);

        z_lerp_1 + fz * (x_lerp_21 + fy * (ab2 + fx * (bb2 - ab2) - x_lerp_21) - z_lerp_1)
    }

    fn simplex2d(&self, x: f64, y: f64) -> f64 {
        let x = x + self.offset_x;
        let y = y + self.offset_y;

        let s = (x + y) * F2;
        let i = (x + s) as i32;
        let j = (y + s) as i32;
        let t = (i + j) as f64 * G2;
        let x0 = x - (i as f64 - t);
        let y0 = y - (j as f64 - t);

        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };

        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        let x2 = x0 + G22;
        let y2 = y0 + G22;

        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;

        let mut n = 0.0;

        let t0 = 0.5 - x0 * x0 - y0 * y0;
        if t0 > 0.0 {
            let gi = self.perm[ii + self.perm[jj] as usize] % 12;
            n += t0 * t0 * t0 * t0 * dot(&GRAD3[gi as usize], x0, y0);
        }

        let t1 = 0.5 - x1 * x1 - y1 * y1;
        if t1 > 0.0 {
            let gi = self.perm[ii + i1 as usize + self.perm[jj + j1 as usize] as usize] % 12;
            n += t1 * t1 * t1 * t1 * dot(&GRAD3[gi as usize], x1, y1);
        }

        let t2 = 0.5 - x2 * x2 - y2 * y2;
        if t2 > 0.0 {
            let gi = self.perm[ii + 1 + self.perm[jj + 1] as usize] % 12;
            n += t2 * t2 * t2 * t2 * dot(&GRAD3[gi as usize], x2, y2);
        }

        70.0 * n
    }

    fn simplex3d(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.offset_x;
        let y = y + self.offset_y;
        let z = z + self.offset_z;

        let s = (x + y + z) * F3;
        let i = (x + s) as i32;
        let j = (y + s) as i32;
        let k = (z + s) as i32;
        let t = (i + j + k) as f64 * G3;
        let x0 = x - (i as f64 - t);
        let y0 = y - (j as f64 - t);
        let z0 = z - (k as f64 - t);

        let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
            if y0 >= z0 {
                (1, 0, 0, 1, 1, 0)
            } else if x0 >= z0 {
                (1, 0, 0, 1, 0, 1)
            } else {
                (0, 0, 1, 1, 0, 1)
            }
        } else if y0 < z0 {
            (0, 0, 1, 0, 1, 1)
        } else if x0 < z0 {
            (0, 1, 0, 0, 1, 1)
        } else {
            (0, 1, 0, 1, 1, 0)
        };

        let x1 = x0 - i1 as f64 + G3;
        let y1 = y0 - j1 as f64 + G3;
        let z1 = z0 - k1 as f64 + G3;
        let x2 = x0 - i2 as f64 + 2.0 * G3;
        let y2 = y0 - j2 as f64 + 2.0 * G3;
        let z2 = z0 - k2 as f64 + 2.0 * G3;
        let x3 = x0 - 1.0 + 3.0 * G3;
        let y3 = y0 - 1.0 + 3.0 * G3;
        let z3 = z0 - 1.0 + 3.0 * G3;

        let ii = (i & 255) as usize;
        let jj = (j & 255) as usize;
        let kk = (k & 255) as usize;

        let mut n = 0.0;

        let t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
        if t0 > 0.0 {
            let gi = self.perm[ii + self.perm[jj + self.perm[kk] as usize] as usize] % 12;
            n += t0 * t0 * t0 * t0 * dot3(&GRAD3[gi as usize], x0, y0, z0);
        }

        let t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
        if t1 > 0.0 {
            let gi = self.perm
                [ii + i1 as usize + self.perm[jj + j1 as usize + self.perm[kk + k1 as usize] as usize] as usize]
                % 12;
            n += t1 * t1 * t1 * t1 * dot3(&GRAD3[gi as usize], x1, y1, z1);
        }

        let t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
        if t2 > 0.0 {
            let gi = self.perm
                [ii + i2 as usize + self.perm[jj + j2 as usize + self.perm[kk + k2 as usize] as usize] as usize]
                % 12;
            n += t2 * t2 * t2 * t2 * dot3(&GRAD3[gi as usize], x2, y2, z2);
        }

        let t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
        if t3 > 0.0 {
            let gi = self.perm
                [ii + 1 + self.perm[jj + 1 + self.perm[kk + 1] as usize] as usize]
                % 12;
            n += t3 * t3 * t3 * t3 * dot3(&GRAD3[gi as usize], x3, y3, z3);
        }

        32.0 * n
    }
}

fn dot(g: &[f64; 3], x: f64, y: f64) -> f64 {
    g[0] * x + g[1] * y
}

fn dot3(g: &[f64; 3], x: f64, y: f64, z: f64) -> f64 {
    g[0] * x + g[1] * y + g[2] * z
}

// Nukkit 的 grad：低 4 位选轴 与经典 Perlin 的 12 方向表等价
fn grad(hash: i32, x: f64, y: f64, z: f64) -> f64 {
    let hash = hash & 15;
    let u = if hash < 8 { x } else { y };
    let v = if hash < 4 {
        y
    } else if hash == 12 || hash == 14 {
        x
    } else {
        z
    };
    let a = if hash & 1 == 0 { u } else { -u };
    let b = if hash & 2 == 0 { v } else { -v };
    a + b
}

// 对齐 Generator.getFastNoise2D：稀疏采样后双线性插值
#[allow(clippy::too_many_arguments)]
pub fn fast_noise2d(
    noise: &Noise,
    x_size: usize,
    z_size: usize,
    sampling_rate: usize,
    x: i32,
    y: i32,
    z: i32,
    x_zoom: u32,
    z_zoom: u32,
) -> Vec<f64> {
    let stride = z_size + 1;
    let mut array = vec![0.0; (x_size + 1) * stride];

    let mut xx = 0;
    while xx <= x_size {
        let mut zz = 0;
        while zz <= z_size {
            array[xx * stride + zz] = noise.noise3d(
                ((x + xx as i32) >> x_zoom) as f64,
                y as f64,
                ((z + zz as i32) >> z_zoom) as f64,
                false,
            );
            zz += sampling_rate;
        }
        xx += sampling_rate;
    }

    for xx in 0..x_size {
        for zz in 0..z_size {
            if xx % sampling_rate == 0 && zz % sampling_rate == 0 {
                continue;
            }
            let nx = xx / sampling_rate * sampling_rate;
            let nz = zz / sampling_rate * sampling_rate;
            array[xx * stride + zz] = bilinear_lerp(
                xx as f64,
                zz as f64,
                array[nx * stride + nz],
                array[nx * stride + nz + sampling_rate],
                array[(nx + sampling_rate) * stride + nz],
                array[(nx + sampling_rate) * stride + nz + sampling_rate],
                nx as f64,
                (nx + sampling_rate) as f64,
                nz as f64,
                (nz + sampling_rate) as f64,
            );
        }
    }

    array
}

// 对齐 Generator.getFastNoise3D：索引顺序是 x z y
#[allow(clippy::too_many_arguments)]
pub fn fast_noise3d(
    noise: &Noise,
    x_size: usize,
    y_size: usize,
    z_size: usize,
    x_rate: usize,
    y_rate: usize,
    z_rate: usize,
    x: i32,
    y: i32,
    z: i32,
) -> Vec<f64> {
    let y_stride = y_size + 1;
    let index = |xx: usize, zz: usize, yy: usize| (xx * (z_size + 1) + zz) * y_stride + yy;
    let mut array = vec![0.0; (x_size + 1) * (z_size + 1) * y_stride];

    let mut xx = 0;
    while xx <= x_size {
        let mut zz = 0;
        while zz <= z_size {
            let mut yy = 0;
            while yy <= y_size {
                array[index(xx, zz, yy)] = noise.noise3d(
                    (x + xx as i32) as f64,
                    (y + yy as i32) as f64,
                    (z + zz as i32) as f64,
                    true,
                );
                yy += y_rate;
            }
            zz += z_rate;
        }
        xx += x_rate;
    }

    // 插值权重只取决于坐标对采样率的余数 预先算成表 免得每格做六次除法
    let weights = |rate: usize| -> Vec<(f64, f64)> {
        (0..rate)
            .map(|r| ((rate - r) as f64 / rate as f64, r as f64 / rate as f64))
            .collect()
    };
    let x_weight = weights(x_rate);
    let y_weight = weights(y_rate);
    let z_weight = weights(z_rate);

    for xx in 0..x_size {
        let (dx1, dx2) = x_weight[xx % x_rate];
        for zz in 0..z_size {
            let (dz1, dz2) = z_weight[zz % z_rate];
            let nx = xx / x_rate * x_rate;
            let nz = zz / z_rate * z_rate;
            let nnx = nx + x_rate;
            let nnz = nz + z_rate;

            // 八个角点在行内的起址 只与 xx zz 有关 提到 yy 循环外
            let row = (xx * (z_size + 1) + zz) * y_stride;
            let q00 = (nx * (z_size + 1) + nz) * y_stride;
            let q10 = (nnx * (z_size + 1) + nz) * y_stride;
            let q01 = (nx * (z_size + 1) + nnz) * y_stride;
            let q11 = (nnx * (z_size + 1) + nnz) * y_stride;

            for yy in 0..y_size {
                if xx % x_rate == 0 && zz % z_rate == 0 && yy % y_rate == 0 {
                    continue;
                }
                let ny = yy / y_rate * y_rate;
                let nny = ny + y_rate;

                let (dy1, dy2) = y_weight[yy % y_rate];

                array[row + yy] = dz1
                    * (dy1 * (dx1 * array[q00 + ny] + dx2 * array[q10 + ny])
                        + dy2 * (dx1 * array[q00 + nny] + dx2 * array[q10 + nny]))
                    + dz2
                        * (dy1 * (dx1 * array[q01 + ny] + dx2 * array[q11 + ny])
                            + dy2 * (dx1 * array[q01 + nny] + dx2 * array[q11 + nny]));
            }
        }
    }

    array
}

#[allow(clippy::too_many_arguments)]
fn bilinear_lerp(
    x: f64,
    y: f64,
    q00: f64,
    q01: f64,
    q10: f64,
    q11: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
) -> f64 {
    let dx1 = (x2 - x) / (x2 - x1);
    let dx2 = (x - x1) / (x2 - x1);

    ((y2 - y) / (y2 - y1)) * (dx1 * q00 + dx2 * q10)
        + ((y - y1) / (y2 - y1)) * (dx1 * q01 + dx2 * q11)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_noise(kind: NoiseKind, seed: i64) -> Noise {
        let mut random = NukkitRandom::new(seed);
        Noise::new(kind, &mut random, 2.0, 1.0 / 8.0, 1.0 / 512.0)
    }

    #[test]
    fn same_seed_same_noise() {
        let a = random_noise(NoiseKind::Simplex, 42);
        let b = random_noise(NoiseKind::Simplex, 42);
        for i in 0..32 {
            let x = i as f64 * 7.3;
            assert_eq!(a.noise3d(x, 0.0, x * 1.7, false), b.noise3d(x, 0.0, x * 1.7, false));
        }
    }

    #[test]
    fn different_seed_differs() {
        let a = random_noise(NoiseKind::Perlin, 42);
        let b = random_noise(NoiseKind::Perlin, 43);
        let mut differs = false;
        for i in 0..32 {
            let x = i as f64 * 3.1;
            if (a.noise3d(x, 0.0, 0.0, false) - b.noise3d(x, 0.0, 0.0, false)).abs() > 1e-9 {
                differs = true;
                break;
            }
        }
        assert!(differs);
    }

    // 噪声连续 相邻采样不跳变
    #[test]
    fn simplex_is_continuous() {
        let noise = random_noise(NoiseKind::Simplex, 7);
        let mut last = noise.noise3d(0.0, 0.0, 0.0, false);
        for i in 1..200 {
            let x = i as f64 * 0.05;
            let value = noise.noise3d(x, 0.0, 0.0, false);
            assert!((value - last).abs() < 0.5, "x={x} {last} -> {value}");
            last = value;
        }
    }

    #[test]
    fn fast_noise2d_matches_direct_sampling_on_grid() {
        let noise = random_noise(NoiseKind::Simplex, 3);
        let array = fast_noise2d(&noise, 16, 16, 4, 0, 0, 0, 0, 0);
        let stride = 17;
        for x in [0usize, 4, 8, 12, 16] {
            for z in [0usize, 4, 8, 12, 16] {
                let direct = noise.noise3d(x as f64, 0.0, z as f64, false);
                assert!((array[x * stride + z] - direct).abs() < 1e-12);
            }
        }
    }

    // 插值点落在相邻采样之间
    #[test]
    fn fast_noise2d_interpolates_between_samples() {
        let noise = random_noise(NoiseKind::Simplex, 11);
        let array = fast_noise2d(&noise, 16, 16, 4, 0, 0, 0, 0, 0);
        let stride = 17;
        let mid = array[2 * stride + 2];
        let low = array[0];
        let high = array[4 * stride + 4];
        let min = low.min(high);
        let max = low.max(high);
        assert!(mid >= min - 1e-9 && mid <= max + 1e-9, "{min} {mid} {max}");
    }
}
