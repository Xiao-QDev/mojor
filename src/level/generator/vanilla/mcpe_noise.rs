//! MCPE 0.14.3 地形噪声引擎（按 libminecraftpe.so 反汇编还原）。
//!
//! 与 `example/mcpe_reader/mcpe_noise.py` 逐点等价：
//! * `Random` 是 `std::mt19937`，`nextFloat() = (f32)(u32 * 2^-32)`
//! * `nextInt(n)` 是无符号取模 `u32 % n`（不是 Java 的 LCG）
//! * `ImprovedNoise`：3 个 offset = `nextFloat()*256`，再 256 次 `nextInt(256-i)` 洗牌
//! * 多八度第 i 层：坐标与 scale 都乘 `0.5^i`，输出乘 `2^i`（div 约定）

pub struct McpeRandom {
    mt: [u32; 624],
    mti: usize,
}

impl McpeRandom {
    pub fn new(seed: u32) -> Self {
        let mut mt = [0u32; 624];
        mt[0] = seed;
        for i in 1..624 {
            let prev = mt[i - 1];
            mt[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        Self { mt, mti: 624 }
    }

    /// 与 libstdc++ `_M_gen_rand` 一致：**原地**更新，
    /// i>=227 时引用的是已被本轮改写过的 `mt[i-227]`（不是原值）。
    fn twist(&mut self) {
        for i in 0..624 {
            let y = (self.mt[i] & 0x8000_0000) | (self.mt[(i + 1) % 624] & 0x7fff_ffff);
            let mut v = self.mt[(i + 397) % 624] ^ (y >> 1);
            if y & 1 != 0 {
                v ^= 0x9908_b0df;
            }
            self.mt[i] = v;
        }
        self.mti = 0;
    }

    pub fn genrand(&mut self) -> u32 {
        if self.mti >= 624 {
            self.twist();
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `vcvt.f64.u32 -> *2^-32 -> vcvt.f32.f64`：中间落地成 f32
    pub fn next_float(&mut self) -> f64 {
        (self.genrand() as f64 / 4_294_967_296.0) as f32 as f64
    }

    pub fn next_int(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.genrand() % n
        }
    }
}

#[derive(Clone)]
pub struct ImprovedNoise {
    xo: f64,
    yo: f64,
    zo: f64,
    p: [u8; 512],
}

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn grad(hash: usize, x: f64, y: f64, z: f64) -> f64 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 != 0 { -u } else { u }) + (if h & 2 != 0 { -v } else { v })
}

impl ImprovedNoise {
    pub fn new(rnd: &mut McpeRandom) -> Self {
        let xo = rnd.next_float() * 256.0;
        let yo = rnd.next_float() * 256.0;
        let zo = rnd.next_float() * 256.0;
        let mut perm = [0u8; 256];
        for (i, v) in perm.iter_mut().enumerate() {
            *v = i as u8;
        }
        for i in 0..256usize {
            let j = i + rnd.next_int((256 - i) as u32) as usize;
            perm.swap(i, j);
        }
        let mut p = [0u8; 512];
        p[..256].copy_from_slice(&perm);
        p[256..].copy_from_slice(&perm);
        Self { xo, yo, zo, p }
    }

    /// 坐标为“世界坐标 + offset”，与 Python `read_area_raw` 的合成方式一致
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.xo;
        let y = y + self.yo;
        let z = z + self.zo;
        let xf0 = x.floor();
        let yf0 = y.floor();
        let zf0 = z.floor();
        let dx = x - xf0;
        let dy = y - yf0;
        let dz = z - zf0;
        let ix = (xf0 as i64 & 255) as usize;
        let iy = (yf0 as i64 & 255) as usize;
        let iz = (zf0 as i64 & 255) as usize;
        let u = fade(dx);
        let v = fade(dy);
        let w = fade(dz);
        let p = |i: usize| self.p[i] as usize;
        let a = p(ix) + iy;
        let b = p(ix + 1) + iy;
        let aa = p(a) + iz;
        let ab = p(a + 1) + iz;
        let ba = p(b) + iz;
        let bb = p(b + 1) + iz;
        let l1 = |t: f64, a: f64, b: f64| a + t * (b - a);
        let x1 = l1(u, grad(p(aa), dx, dy, dz), grad(p(ba), dx - 1.0, dy, dz));
        let x2 = l1(u, grad(p(ab), dx, dy - 1.0, dz), grad(p(bb), dx - 1.0, dy - 1.0, dz));
        let y1 = l1(v, x1, x2);
        let x3 = l1(u, grad(p(aa + 1), dx, dy, dz - 1.0),
                  grad(p(ba + 1), dx - 1.0, dy, dz - 1.0));
        let x4 = l1(u, grad(p(ab + 1), dx, dy - 1.0, dz - 1.0),
                  grad(p(bb + 1), dx - 1.0, dy - 1.0, dz - 1.0));
        let y2 = l1(v, x3, x4);
        l1(w, y1, y2)
    }
}

#[derive(Clone)]
pub struct OctavesNoise {
    pub levels: Vec<ImprovedNoise>,
}

impl OctavesNoise {
    pub fn new(rnd: &mut McpeRandom, octaves: usize) -> Self {
        Self {
            levels: (0..octaves).map(|_| ImprovedNoise::new(rnd)).collect(),
        }
    }

    /// 下标 `x * (sz*sy) + z * sy + y`，与 Python 的 `(sx, sz, sy)` 形状一致
    pub fn get_region(
        &self,
        pos: (f64, f64, f64),
        sx: usize,
        sy: usize,
        sz: usize,
        scale: (f64, f64, f64),
    ) -> Vec<f64> {
        let mut out = vec![0.0f64; sx * sz * sy];
        let mut amp = 1.0f64;
        for lvl in &self.levels {
            let inv = 1.0 / amp;
            for i in 0..sx {
                let x = (pos.0 + i as f64) * scale.0 * amp;
                for k in 0..sz {
                    let z = (pos.2 + k as f64) * scale.2 * amp;
                    let base = (i * sz + k) * sy;
                    for j in 0..sy {
                        let y = (pos.1 + j as f64) * scale.1 * amp;
                        out[base + j] += lvl.sample(x, y, z) * inv;
                    }
                }
            }
            amp *= 0.5;
        }
        out
    }
}

/// MCPE 的 7 个噪声对象（构造顺序即随机数消耗顺序）
#[derive(Clone)]
pub struct NoiseStack {
    pub min_limit: OctavesNoise,   // n16a, 16 八度, scale 684.412
    pub max_limit: OctavesNoise,   // n16b, 16 八度, scale 684.412
    pub main_noise: OctavesNoise,  // n8,   8 八度,  scale 684.412/80 · /160 · /80
    pub surface: OctavesNoise,     // n4,   4 八度,  scale 0.0625（地表）
    pub scale_noise: OctavesNoise, // n10,  10 八度, scale 1.121
    pub depth_noise: OctavesNoise, // n16c, 16 八度, scale 200
    pub mob_spawner: OctavesNoise, // n8b,  8 八度
}

impl NoiseStack {
    pub fn new(seed: u32) -> Self {
        let mut r = McpeRandom::new(seed);
        let min_limit = OctavesNoise::new(&mut r, 16);
        let max_limit = OctavesNoise::new(&mut r, 16);
        let main_noise = OctavesNoise::new(&mut r, 8);
        let surface = OctavesNoise::new(&mut r, 4);
        let scale_noise = OctavesNoise::new(&mut r, 10);
        let depth_noise = OctavesNoise::new(&mut r, 16);
        let mob_spawner = OctavesNoise::new(&mut r, 8);
        Self {
            min_limit,
            max_limit,
            main_noise,
            surface,
            scale_noise,
            depth_noise,
            mob_spawner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt19937_matches_python() {
        let mut r = McpeRandom::new(1_152_860_609);
        let got: Vec<u32> = (0..4).map(|_| r.genrand()).collect();
        assert_eq!(got, vec![1733503684, 3779099160, 3616625770, 2648414837]);

        let mut r = McpeRandom::new(1_152_860_609);
        let f: Vec<f64> = (0..3).map(|_| r.next_float()).collect();
        assert!((f[0] - 0.403612792).abs() < 1e-9, "{f:?}");
        assert!((f[1] - 0.879890084).abs() < 1e-9, "{f:?}");
        assert!((f[2] - 0.842061281).abs() < 1e-9, "{f:?}");

        let mut r = McpeRandom::new(1_152_860_609);
        let i: Vec<u32> = (0..4).map(|_| r.next_int(1000)).collect();
        assert_eq!(i, vec![684, 160, 770, 837]);
    }

    /// 跨越 624 边界后的值，检验 twist 是否为原地更新
    #[test]
    fn mt19937_twist_matches_python() {
        let mut r = McpeRandom::new(1_152_860_609);
        let v: Vec<u32> = (0..700).map(|_| r.genrand()).collect();
        assert_eq!(&v[600..604], &[3223613968, 1713635667, 350986015, 2813329668]);
        assert_eq!(v[699], 3894413063);
    }
}
