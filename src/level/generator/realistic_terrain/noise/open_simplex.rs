//! 对应 `rtg/noise/OpenSimplexNoise.php`。
//!
//! RTG+ 的查表版 OpenSimplex（KdotJPG / DigitalShadow / srs_bsns，AGPLv3）。
//! 三处必须逐字照抄，改动任何一处都会让地形整体变形：
//!
//! 1. 置换表初始化用 `seed*6364136223846793005 + 1442695040888963407` 推进（64 位回绕）
//! 2. `index` 的三个位段位移量是 2 / 3 / 4
//! 3. `multi_eval2d` 里 `attn` **不自乘**（`noise2d` 里自乘），衰减次数不同

use crate::level::generator::realistic_terrain::noise_tables as t;

pub struct OpenSimplexNoise {
    perm: [i32; 1024],
    perm2d: [i32; 1024],
    perm_sph: [i32; 1024],
    perm3d: [i32; 1024],
}

impl OpenSimplexNoise {
    pub fn new(seed: i64) -> Self {
        let mut source: [i32; 1024] = core::array::from_fn(|i| i as i32);
        let mut perm = [0i32; 1024];
        let mut perm2d = [0i32; 1024];
        let mut perm_sph = [0i32; 1024];
        let mut perm3d = [0i32; 1024];
        let mut seed = seed;

        for i in (0..1024usize).rev() {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut r = seed.wrapping_add(31) % (i as i64 + 1);
            if r < 0 {
                r += i as i64 + 1;
            }
            let p = source[r as usize];
            perm[i] = p;
            perm2d[i] = p % 12 * 2;
            perm_sph[i] = (p / 12) % 12 * 2;
            perm3d[i] = p % 48 * 3;
            source[r as usize] = source[i];
        }

        Self {
            perm,
            perm2d,
            perm_sph,
            perm3d,
        }
    }

    /// 二维噪声，输入输出都按 float32 处理（原 `noise2f`）。
    pub fn noise2f(&self, x: f32, y: f32) -> f32 {
        self.noise2d(x as f64, y as f64) as f32
    }

    /// 三维噪声，输入输出都按 float32 处理（原 `noise3f`）。
    pub fn noise3f(&self, x: f32, y: f32, z: f32) -> f32 {
        self.noise3d(x as f64, y as f64, z as f64) as f32
    }

    pub fn noise2d(&self, x: f64, y: f64) -> f64 {
        let s = -0.211_324_865_405_187 * (x + y);
        let xs = x + s;
        let ys = y + s;
        let xb = xs.floor() as i32;
        let yb = ys.floor() as i32;
        let xi = xs - xb as f64;
        let yi = ys - yb as f64;

        let a = (yi - xi + 1.0) as i32;
        let index = (a << 2)
            | (((xi + yi / 2.0 + a as f64 / 2.0) as i32) << 3)
            | (((yi + xi / 2.0 + 0.5 - a as f64 / 2.0) as i32) << 4);

        let s = (xi + yi) * 0.366_025_403_784_439;
        let xi = xi + s;
        let yi = yi + s;

        let mut value = 0.0;
        for i in 0..4 {
            let p = t::LOOKUP_2D[(index + i) as usize];
            let dx = xi + p.dx;
            let dy = yi + p.dy;
            let mut attn = 2.0 - dx * dx - dy * dy;
            if attn <= 0.0 {
                continue;
            }
            let gp = (self.perm[((xb + p.x) & 1023) as usize] ^ ((yb + p.y) & 1023)) as usize;
            let gi = self.perm2d[gp] as usize;
            let extrap = t::GRADIENTS_2D[gi] * dx + t::GRADIENTS_2D[gi + 1] * dy;
            attn *= attn;
            value += attn * attn * extrap;
        }
        value
    }

    /// 返回 `[deltaX, deltaY]`。`derivative=false` 时是球面梯度变体，
    /// `true` 时是解析导数。两者共用同一套格子选取。
    pub fn multi_eval2d(&self, x: f64, y: f64, derivative: bool) -> [f64; 2] {
        let s = -0.211_324_865_405_187 * (x + y);
        let xs = x + s;
        let ys = y + s;
        let xb = xs.floor() as i32;
        let yb = ys.floor() as i32;
        let xi = xs - xb as f64;
        let yi = ys - yb as f64;

        let a = (yi - xi + 1.0) as i32;
        let index = (a << 2)
            | (((xi + yi / 2.0 + a as f64 / 2.0) as i32) << 3)
            | (((yi + xi / 2.0 + 0.5 - a as f64 / 2.0) as i32) << 4);

        let s = (xi + yi) * 0.366_025_403_784_439;
        let xi = xi + s;
        let yi = yi + s;

        let mut rx = 0.0;
        let mut ry = 0.0;
        for i in 0..4 {
            let p = t::LOOKUP_2D[(index + i) as usize];
            let dx = xi + p.dx;
            let dy = yi + p.dy;
            let attn = 2.0 - dx * dx - dy * dy;
            if attn <= 0.0 {
                continue;
            }
            let gp = (self.perm[((xb + p.x) & 1023) as usize] ^ ((yb + p.y) & 1023)) as usize;
            let gi = self.perm2d[gp] as usize;
            let gx = t::GRADIENTS_2D[gi];
            let gy = t::GRADIENTS_2D[gi + 1];
            let extrap = gx * dx + gy * dy;
            let sq = attn * attn;
            if derivative {
                rx += (gx * attn - 8.0 * dx * extrap) * sq * attn;
                ry += (gy * attn - 8.0 * dy * extrap) * sq * attn;
            } else {
                let gi = self.perm_sph[gp] as usize;
                let extrap = sq * sq * extrap;
                rx += extrap * t::GRADIENTS_SPH2[gi];
                ry += extrap * t::GRADIENTS_SPH2[gi + 1];
            }
        }
        [rx, ry]
    }

    pub fn noise3d(&self, x: f64, y: f64, z: f64) -> f64 {
        let s = (x + y + z) * (-1.0 / 6.0);
        let xs = x + s;
        let ys = y + s;
        let zs = z + s;
        let xb = xs.floor() as i32;
        let yb = ys.floor() as i32;
        let zb = zs.floor() as i32;

        let s = (xb + yb + zb) as f64 * (1.0 / 3.0);
        let dx0 = x - (xb as f64 + s);
        let dy0 = y - (yb as f64 + s);
        let dz0 = z - (zb as f64 + s);

        let xi = xs - xb as f64;
        let yi = ys - yb as f64;
        let zi = zs - zb as f64;
        let sum = xi + yi + zi;

        let hash = (yi - zi + 1.0) as i32
            | (((xi - yi + 1.0) as i32) << 1)
            | (((xi - zi + 1.0) as i32) << 2)
            | ((sum as i32) << 3)
            | (((sum + zi) as i32) << 5)
            | (((sum + yi) as i32) << 7)
            | (((sum + xi) as i32) << 9);

        let mut value = 0.0;
        if hash < 0 || hash as usize >= t::LOOKUP_3D.len() {
            return value;
        }
        let lookup = t::LOOKUP_3D[hash as usize];
        if lookup < 0 {
            return value;
        }

        for c in t::CONTRIBUTIONS_3D[lookup as usize] {
            let dx = dx0 + c.d[0];
            let dy = dy0 + c.d[1];
            let dz = dz0 + c.d[2];
            let mut attn = 2.0 - dx * dx - dy * dy - dz * dz;
            if attn <= 0.0 {
                continue;
            }
            let inner =
                (self.perm[((xb + c.p[0]) & 1023) as usize] ^ ((yb + c.p[1]) & 1023)) & 1023;
            let mid = (self.perm[inner as usize] ^ ((zb + c.p[2]) & 1023)) & 1023;
            let gi = self.perm3d[mid as usize] as usize;
            let v = t::GRADIENTS_3D[gi] * dx
                + t::GRADIENTS_3D[gi + 1] * dy
                + t::GRADIENTS_3D[gi + 2] * dz;
            attn *= attn;
            value += attn * attn * v;
        }
        value
    }
}
