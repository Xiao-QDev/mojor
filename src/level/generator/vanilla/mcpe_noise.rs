//! MCPE 0.14.3 地形噪声引擎（按 libminecraftpe.so 反汇编还原）。
//!
//! 与 `example/mcpe_reader/mcpe_noise.py` 逐点等价：
//! * `Random` 是 `std::mt19937`，`nextFloat() = (f32)(u32 * 2^-32)`
//! * `nextInt(n)` 是无符号取模 `u32 % n`（不是 Java 的 LCG）
//! * `ImprovedNoise`：3 个 offset = `nextFloat()*256`，再 256 次 `nextInt(256-i)` 洗牌
//! * 多八度第 i 层：坐标与 scale 都乘 `0.5^i`，输出乘 `2^i`（div 约定）

#[derive(Clone)]
pub struct McpeRandom {
    mt: [u32; 624],
    mti: usize,
}

impl McpeRandom {
    /// 对拍用：MT 状态下标
    pub fn mti(&self) -> usize {
        self.mti
    }

    /// 全零状态（genrand 恒为 0）。BlockTickingQueue 的 Random 原版用时间做种子，
    /// 只影响岩浆 tick 的延迟；这里取确定的全零，与模拟器对拍时零初始化的队列一致。
    pub fn zero_state() -> Self {
        Self { mt: [0u32; 624], mti: 624 }
    }

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
    /// `(float)((double)genrand * 2^-32)`
    pub fn next_float(&mut self) -> f32 {
        (self.genrand() as f64 * (1.0 / 4_294_967_296.0)) as f32
    }

    /// `nextBoolean()`：genrand 的 0x08000000 位
    pub fn next_boolean(&mut self) -> bool {
        self.genrand() & 0x0800_0000 != 0
    }

    pub fn next_int(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.genrand() % n
        }
    }
}

// ---------------------------------------------------------------------------
// 以下逐条照 x86 libminecraftpe.so 的实现移植，全部用 f32、保持同样的运算顺序，
// 以保证与原版逐位一致（已在 unicorn 里直接调用原函数对拍）：
//   ImprovedNoise::_calcValues  @0x7c5130   ImprovedNoise::_grad   @0x7c50b0
//   ImprovedNoise::_grad2       @0x7c5020   ImprovedNoise::_lerp   @0x7c4ff0
//   ImprovedNoise::_blendCubeCorners @0x7c51e0
//   ImprovedNoise::_readArea    @0x7c56e0（含 Java 的 y 缓存怪癖与 ySize==1 的 2D 分支）
//   PerlinNoise::getRegion      @0x7c6890 / 2D 包装 @0x7c6ad0
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ImprovedNoise {
    xo: f32,
    yo: f32,
    zo: f32,
    p: [i32; 512],
}

/// `_calcValues(v, &i, &fade)`：返回 (i & 255, 小数部分, fade)
#[inline]
fn calc(v: f32) -> (i32, f32, f32) {
    let mut i = v as i32; // cvttss2si
    let mut fi = i as f32;
    if fi > v {
        i -= 1;
        fi = i as f32;
    }
    let f = v - fi;
    let t = ((6.0f32 * f + -15.0) * f + 10.0) * f;
    (i & 255, f, (f * f) * t)
}

#[inline]
fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (b - a) * t + a
}

#[inline]
fn grad(hash: i32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15;
    let mut u = if h < 8 { x } else { y };
    let mut v = if h < 4 {
        y
    } else if hash & 13 == 12 {
        x
    } else {
        z
    };
    if hash & 1 != 0 {
        u = -u;
    }
    if hash & 2 != 0 {
        v = -v;
    }
    v + u
}

#[inline]
fn grad2(hash: i32, x: f32, z: f32) -> f32 {
    let mut u = ((1 - ((hash & 8) >> 3)) as f32) * x;
    let mut v = if hash & 15 <= 3 {
        0.0
    } else if hash & 13 == 12 {
        x
    } else {
        z
    };
    if hash & 1 != 0 {
        u = -u;
    }
    if hash & 2 != 0 {
        v = -v;
    }
    u + v
}

impl ImprovedNoise {
    pub fn new(rnd: &mut McpeRandom) -> Self {
        let xo = rnd.next_float() * 256.0;
        let yo = rnd.next_float() * 256.0;
        let zo = rnd.next_float() * 256.0;
        let mut p = [0i32; 512];
        for (i, v) in p.iter_mut().take(256).enumerate() {
            *v = i as i32;
        }
        for i in 0..256usize {
            let j = i + rnd.next_int((256 - i) as u32) as usize;
            p.swap(i, j);
            p[i + 256] = p[i];
        }
        Self { xo, yo, zo, p }
    }

    #[inline]
    fn pp(&self, i: i32) -> i32 {
        self.p[i as usize]
    }

    /// `_blendCubeCorners`：返回 (a, b, c, d) = 四条 x 向插值
    #[inline]
    fn blend(&self, fx: f32, fy: f32, fz: f32, ix: i32, iy: i32, iz: i32, tx: f32) -> (f32, f32, f32, f32) {
        let a_ = self.pp(ix) + iy;
        let aa = self.pp(a_) + iz;
        let ab = self.pp(a_ + 1) + iz;
        let b_ = self.pp(ix + 1) + iy;
        let ba = self.pp(b_) + iz;
        let bb = self.pp(b_ + 1) + iz;
        let (x1, y1, z1) = (fx - 1.0, fy - 1.0, fz - 1.0);
        let a = lerp(tx, grad(self.pp(aa), fx, fy, fz), grad(self.pp(ba), x1, fy, fz));
        let b = lerp(tx, grad(self.pp(ab), fx, y1, fz), grad(self.pp(bb), x1, y1, fz));
        let c = lerp(tx, grad(self.pp(aa + 1), fx, fy, z1), grad(self.pp(ba + 1), x1, fy, z1));
        let d = lerp(tx, grad(self.pp(ab + 1), fx, y1, z1), grad(self.pp(bb + 1), x1, y1, z1));
        (a, b, c, d)
    }

    /// `_readArea(out, pos, xs, ys, zs, scale, amp)`，out 布局 `[x][z][y]`
    #[allow(clippy::too_many_arguments)]
    pub fn read_area(
        &self,
        out: &mut [f32],
        pos: (f32, f32, f32),
        xs: usize,
        ys: usize,
        zs: usize,
        sc: (f32, f32, f32),
        amp: f32,
    ) {
        let inv = 1.0f32 / amp;
        if ys == 1 {
            let mut idx = 0;
            for i in 0..xs {
                let (ix, fx, tx) = calc((i as f32 + pos.0) * sc.0 + self.xo);
                let x1 = fx - 1.0;
                for k in 0..zs {
                    let (iz, fz, tz) = calc((k as f32 + pos.2) * sc.2 + self.zo);
                    let aa = self.pp(self.pp(ix)) + iz;
                    let ba = self.pp(self.pp(ix + 1)) + iz;
                    let z1 = fz - 1.0;
                    let l1 = lerp(tx, grad2(self.pp(aa), fx, fz), grad(self.pp(ba), x1, 0.0, fz));
                    let l2 = lerp(tx, grad(self.pp(aa + 1), fx, 0.0, z1), grad(self.pp(ba + 1), x1, 0.0, z1));
                    let v = lerp(tz, l1, l2);
                    out[idx] += v * inv;
                    idx += 1;
                }
            }
            return;
        }
        let mut prev = -1i32;
        let (mut a, mut b, mut c, mut d) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut idx = 0;
        for i in 0..xs {
            let (ix, fx, tx) = calc((i as f32 + pos.0) * sc.0 + self.xo);
            for k in 0..zs {
                let (iz, fz, tz) = calc((k as f32 + pos.2) * sc.2 + self.zo);
                for j in 0..ys {
                    let (iy, fy, ty) = calc((j as f32 + pos.1) * sc.1 + self.yo);
                    if j == 0 || iy != prev {
                        prev = iy;
                        (a, b, c, d) = self.blend(fx, fy, fz, ix, iy, iz, tx);
                    }
                    let v1 = lerp(ty, a, b);
                    let v2 = lerp(ty, c, d);
                    let v = lerp(tz, v1, v2);
                    out[idx] += v * inv;
                    idx += 1;
                }
            }
        }
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

    /// `PerlinNoise::getRegion(out, Vec3 pos, xs, ys, zs, Vec3 scale)`，布局 `[x][z][y]`
    pub fn get_region(
        &self,
        pos: (f32, f32, f32),
        xs: usize,
        ys: usize,
        zs: usize,
        scale: (f32, f32, f32),
    ) -> Vec<f32> {
        let mut out = vec![0.0f32; xs * ys * zs];
        let mut amp = 1.0f32;
        for lvl in &self.levels {
            let sc = (scale.0 * amp, scale.1 * amp, scale.2 * amp);
            lvl.read_area(&mut out, pos, xs, ys, zs, sc, amp);
            amp *= 0.5;
        }
        out
    }

    /// `PerlinNoise::getRegion(out, Vec2 pos, xs, zs, Vec2 scale, float)`：
    /// 转成 3D 调用 pos=(x,10,z)、ys=1、scale=(sx,1,sz)，最后的 float 参数不使用
    pub fn get_region_2d(&self, pos: (f32, f32), xs: usize, zs: usize, scale: (f32, f32)) -> Vec<f32> {
        self.get_region((pos.0, 10.0, pos.1), xs, 1, zs, (scale.0, 1.0, scale.1))
    }
}

// ---------------------------------------------------------------------------
// SimplexNoise / PerlinSimplexNoise（地表噪声 +0x2c）
//   SimplexNoise::SimplexNoise(Random&) @0x7c6eb0（初始化同 ImprovedNoise）
//   SimplexNoise::_getValue(Vec2)       @0x7efb20
//   SimplexNoise::_add(...)             @0x7eff00
//   PerlinSimplexNoise::getRegion(Vec2) @0x7f0000
// ---------------------------------------------------------------------------

/// `SimplexNoise::F2/G2`（静态初始化写入 bss 的 f32 位模式）
const F2: f32 = f32::from_bits(0x3ebb67ae);
const G2: f32 = f32::from_bits(0x3e58658c);
const GRAD3: [[i32; 3]; 12] = [
    [1, 1, 0], [-1, 1, 0], [1, -1, 0], [-1, -1, 0],
    [1, 0, 1], [-1, 0, 1], [1, 0, -1], [-1, 0, -1],
    [0, 1, 1], [0, -1, 1], [0, 1, -1], [0, -1, -1],
];

#[derive(Clone)]
pub struct SimplexNoise {
    xo: f32,
    yo: f32,
    p: [i32; 512],
}

#[inline]
fn fastfloor(v: f32) -> i32 {
    (v as i32) - (v <= 0.0) as i32
}

impl SimplexNoise {
    /// 与 ImprovedNoise 同样消耗 3 次 nextFloat + 256 次 nextInt，但三个偏移按**逆序**存放：
    /// 第 1 次 → zo、第 2 次 → yo、第 3 次 → xo（已用原版对象内存核对）
    pub fn new(rnd: &mut McpeRandom) -> Self {
        let n = ImprovedNoise::new(rnd);
        Self { xo: n.zo, yo: n.yo, p: n.p }
    }

    #[inline]
    fn pp(&self, i: i32) -> i32 {
        self.p[i as usize]
    }

    /// `_getValue(Vec2)`，含 ×70
    pub fn value2(&self, x: f32, y: f32) -> f32 {
        let s = (y + x) * F2;
        let i = fastfloor(s + x);
        let j = fastfloor(s + y);
        let t = ((i + j) as f32) * G2;
        let y0 = y - (j as f32 - t);
        let x0 = x - (i as f32 - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let y1 = (G2 - j1 as f32) + y0;
        let x1 = (G2 - i1 as f32) + x0;
        let g2m1 = (G2 + G2) + -1.0;
        let y2 = y0 + g2m1;
        let x2 = g2m1 + x0;
        let ii = i & 255;
        let jj = j & 255;
        let gi0 = self.pp(self.pp(jj) + ii) % 12;
        let gi1 = self.pp(self.pp(jj + j1) + ii + i1) % 12;
        let gi2 = self.pp(ii + self.pp(jj + 1) + 1) % 12;
        let corner = |t: f32, g: i32, x: f32, y: f32| -> f32 {
            if t < 0.0 {
                0.0
            } else {
                let t2 = t * t;
                let gg = GRAD3[g as usize];
                (t2 * t2) * ((gg[1] as f32) * y + (gg[0] as f32) * x)
            }
        };
        let n0 = corner(0.5 - (y0 * y0 + x0 * x0), gi0, x0, y0);
        let n1 = corner(0.5 - (y1 * y1 + x1 * x1), gi1, x1, y1);
        let n2 = corner(0.5 - (y2 * y2 + x2 * x2), gi2, x2, y2);
        ((n0 + n1) + n2) * 70.0
    }

    /// `_add(out, x, z, w, h, sx, sz, amp)`，out 布局 `[j*w + k]`
    #[allow(clippy::too_many_arguments)]
    fn add(&self, out: &mut [f32], x: f32, z: f32, w: usize, h: usize, sx: f32, sz: f32, amp: f32) {
        for j in 0..h {
            let yv = (j as f32 + z) * sz + self.yo;
            for k in 0..w {
                let xv = (k as f32 + x) * sx + self.xo;
                out[j * w + k] += self.value2(xv, yv) * amp;
            }
        }
    }
}

#[derive(Clone)]
pub struct PerlinSimplexNoise {
    pub levels: Vec<SimplexNoise>,
}

impl PerlinSimplexNoise {
    pub fn new(rnd: &mut McpeRandom, octaves: usize) -> Self {
        Self {
            levels: (0..octaves).map(|_| SimplexNoise::new(rnd)).collect(),
        }
    }

    /// `getValue(float x, float y)`（@0x7efe30）：Σ value2(x·f, y·f)/f，f 逐级减半，不加偏移
    pub fn get_value(&self, x: f32, y: f32) -> f32 {
        let mut sum = 0.0f32;
        let mut f = 1.0f32;
        for lvl in &self.levels {
            let v = lvl.value2(x * f, y * f);
            sum += v / f;
            f *= 0.5;
        }
        sum
    }

    /// `getRegion(out, Vec2 pos, w, h, Vec2 scale, f1, f2)`，out `[z*w + x]`
    pub fn get_region(&self, pos: (f32, f32), w: usize, h: usize, scale: (f32, f32), f1: f32, f2: f32) -> Vec<f32> {
        let mut out = vec![0.0f32; w * h];
        let (mut m1, mut m2) = (1.0f32, 1.0f32);
        for lvl in &self.levels {
            let amp = 0.55f32 / m2;
            let k = m1 * m2;
            lvl.add(&mut out, pos.0, pos.1, w, h, scale.0 * k, scale.1 * k, amp);
            m1 *= f1;
            m2 *= f2;
        }
        out
    }
}

#[derive(Clone)]
pub struct NoiseStack {
    pub min_limit: OctavesNoise,   // +0x20 PerlinNoise(16)
    pub max_limit: OctavesNoise,   // +0x24 PerlinNoise(16)
    pub main_noise: OctavesNoise,  // +0x28 PerlinNoise(8)
    pub surface: PerlinSimplexNoise, // +0x2c PerlinSimplexNoise(4)
    pub scale_noise: OctavesNoise, // +0x30 PerlinNoise(10)
    pub depth_noise: OctavesNoise, // +0x34 PerlinNoise(16)
    pub mob_spawner: OctavesNoise, // +0x38 PerlinNoise(8)
}

impl NoiseStack {
    pub fn new(seed: u32) -> Self {
        let mut r = McpeRandom::new(seed);
        let min_limit = OctavesNoise::new(&mut r, 16);
        let max_limit = OctavesNoise::new(&mut r, 16);
        let main_noise = OctavesNoise::new(&mut r, 8);
        let surface = PerlinSimplexNoise::new(&mut r, 4);
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
        let f: Vec<f64> = (0..3).map(|_| r.next_float() as f64).collect();
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
