//! MCPE 0.14.3 biome 生成器 —— 移植自 example/mcpe_reader/genlayer17.py 的 build_old 链
//!
//! 链结构（JE 1.7.10 GenLayer，尾部为 MCPE 实测配置）：
//!   Island -> FuzzyZoom -> AddIsland x3 -> RemoveTooMuchOcean -> AddSnow -> AddIsland
//!   -> TempSmooth x3 -> Zoom x2 -> AddIsland -> AddMushroomIsland -> DeepOcean
//!   -> Biome -> Zoom x2 -> BiomeEdge -> Hills
//!   -> Zoom x3 -> RareBiome -> Smooth -> RiverMix -> VoronoiZoom
//!   支线 RiverInit -> Zoom x5 -> River -> Smooth
//!
//! 实测配置（真实存档 16724 chunk / 428 万格对比）：
//!   BIOME_ZOOM=0、RIVER_ZOOMS=5、TAIL_ZOOMS=3、AddSnow 不出冰原。
//! 最终层分辨率实测为 1 block ≈ 8 cells（dbg_blockcons.py r=8 众数占比 96.7%），
//! 因此 chunk 采样用 get(cx*128, cz*128, 128, 128) 再按 8x8 取众数。

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const A: u64 = 6364136223846793005;
const B: u64 = 1442695040888963407;

// ---------- biome ids ----------
#[allow(dead_code)]
const OCEAN: i32 = 0;
#[allow(dead_code)]
const PLAINS: i32 = 1;
#[allow(dead_code)]
const DESERT: i32 = 2;
#[allow(dead_code)]
const EXTREME_HILLS: i32 = 3;
#[allow(dead_code)]
const FOREST: i32 = 4;
#[allow(dead_code)]
const TAIGA: i32 = 5;
#[allow(dead_code)]
const SWAMP: i32 = 6;
#[allow(dead_code)]
const RIVER: i32 = 7;
#[allow(dead_code)]
const FROZEN_OCEAN: i32 = 10;
#[allow(dead_code)]
const FROZEN_RIVER: i32 = 11;
#[allow(dead_code)]
const ICE_PLAINS: i32 = 12;
#[allow(dead_code)]
const ICE_MOUNTAINS: i32 = 13;
#[allow(dead_code)]
const MUSHROOM_ISLAND: i32 = 14;
#[allow(dead_code)]
const MUSHROOM_SHORE: i32 = 15;
#[allow(dead_code)]
const BEACH: i32 = 16;
#[allow(dead_code)]
const DESERT_HILLS: i32 = 17;
#[allow(dead_code)]
const FOREST_HILLS: i32 = 18;
#[allow(dead_code)]
const TAIGA_HILLS: i32 = 19;
#[allow(dead_code)]
const EXTREME_HILLS_EDGE: i32 = 20;
#[allow(dead_code)]
const JUNGLE: i32 = 21;
#[allow(dead_code)]
const JUNGLE_HILLS: i32 = 22;
#[allow(dead_code)]
const JUNGLE_EDGE: i32 = 23;
#[allow(dead_code)]
const DEEP_OCEAN: i32 = 24;
#[allow(dead_code)]
const STONE_BEACH: i32 = 25;
#[allow(dead_code)]
const COLD_BEACH: i32 = 26;
#[allow(dead_code)]
const BIRCH_FOREST: i32 = 27;
#[allow(dead_code)]
const BIRCH_FOREST_HILLS: i32 = 28;
#[allow(dead_code)]
const ROOFED_FOREST: i32 = 29;
#[allow(dead_code)]
const COLD_TAIGA: i32 = 30;
#[allow(dead_code)]
const COLD_TAIGA_HILLS: i32 = 31;
#[allow(dead_code)]
const MEGA_TAIGA: i32 = 32;
#[allow(dead_code)]
const MEGA_TAIGA_HILLS: i32 = 33;
#[allow(dead_code)]
const EXTREME_HILLS_PLUS: i32 = 34;
#[allow(dead_code)]
const SAVANNA: i32 = 35;
#[allow(dead_code)]
const SAVANNA_PLATEAU: i32 = 36;
#[allow(dead_code)]
const MESA: i32 = 37;
#[allow(dead_code)]
const MESA_PLATEAU_F: i32 = 38;
#[allow(dead_code)]
const MESA_PLATEAU: i32 = 39;

/// AddSnow 是否跳过冰原档：真实存档里 12(icePlains) 和 30(coldTaiga) 均为 0
pub const SNOW_COLD_ONLY: bool = true;

pub trait GenLayer: Send + Sync {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32>;
    fn init_seed(&self, seed: u64);
}

fn is_ocean(v: i32) -> bool {
    v == OCEAN || v == FROZEN_OCEAN || v == DEEP_OCEAN
}

fn is_snow(v: i32) -> bool {
    matches!(
        v,
        ICE_PLAINS | ICE_MOUNTAINS | COLD_TAIGA | COLD_TAIGA_HILLS | FROZEN_OCEAN | FROZEN_RIVER
    )
}

fn is_jungle(v: i32) -> bool {
    matches!(v, JUNGLE | JUNGLE_HILLS | JUNGLE_EDGE)
}

fn is_mesa(v: i32) -> bool {
    matches!(v, MESA | MESA_PLATEAU_F | MESA_PLATEAU)
}

fn is_beach(v: i32) -> bool {
    matches!(v, BEACH | STONE_BEACH | COLD_BEACH)
}

fn has_biome(v: i32) -> bool {
    (0..=39).contains(&v) || (128..=167).contains(&v)
}

/// 1.7.10 BiomeGenBase.isBiomeSimilar 的简化：Mesa 系列互相相似，其余看 id
fn sim(a: i32, b: i32) -> bool {
    a == b || (is_mesa(a) && is_mesa(b))
}

fn mul(a: u64, b: u64) -> u64 {
    a.wrapping_mul(b)
}

// ---------- 随机数：JE 1.7.10 GenLayer 的 LCG ----------
// 用 AtomicU64 而不是 Cell，使整条链满足 Send + Sync（Generator 会被跨线程克隆）。
// BiomeGen 的 clone 会重建整条链，因此实际不会有多线程并发写同一份状态。
struct Lcg {
    d: u64,
    w: AtomicU64,
    c: AtomicU64,
}

impl Lcg {
    fn new(base: i64) -> Self {
        let mut d = base as u64;
        for _ in 0..3 {
            d = mul(d, mul(d, A).wrapping_add(B)).wrapping_add(base as u64);
        }
        Self {
            d,
            w: AtomicU64::new(0),
            c: AtomicU64::new(0),
        }
    }

    fn init_seed(&self, seed: u64) {
        let mut w = seed;
        for _ in 0..3 {
            w = mul(w, mul(w, A).wrapping_add(B)).wrapping_add(self.d);
        }
        self.w.store(w, Ordering::Relaxed);
    }

    #[inline]
    fn init_chunk(&self, x: i32, z: i32) {
        let mut c = self.w.load(Ordering::Relaxed);
        for v in [x, z, x, z] {
            c = mul(c, mul(c, A).wrapping_add(B)).wrapping_add(v as i64 as u64);
        }
        self.c.store(c, Ordering::Relaxed);
    }

    #[inline]
    fn next(&self, n: u32) -> u32 {
        let c = self.c.load(Ordering::Relaxed) as i64;
        let mut v = (c >> 24) % (n as i64);
        if v < 0 {
            v += n as i64;
        }
        let cur = self.c.load(Ordering::Relaxed);
        let w = self.w.load(Ordering::Relaxed);
        self.c.store(
            mul(cur, mul(cur, A).wrapping_add(B)).wrapping_add(w),
            Ordering::Relaxed,
        );
        v as u32
    }
}

/// 生成「先自身、再递归父层」的 init_seed
macro_rules! seed_impl {
    () => {
        fn init_seed(&self, seed: u64) {
            self.lcg.init_seed(seed);
            self.parent.init_seed(seed);
        }
    };
}

// ---------- axm: Island ----------
struct Island {
    lcg: Lcg,
}

impl GenLayer for Island {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                out[i + j * w] = if self.lcg.next(10) == 0 { 1 } else { 0 };
            }
        }
        if x > -(w as i32) && x <= 0 && z > -(h as i32) && z <= 0 {
            out[(-x) as usize + (-z) as usize * w] = 1;
        }
        out
    }
    fn init_seed(&self, seed: u64) {
        self.lcg.init_seed(seed);
    }
}

// ---------- ayc: Zoom ----------
struct Zoom {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
    fuzzy: bool,
}

impl Zoom {
    fn new(base: i64, parent: Arc<dyn GenLayer>, fuzzy: bool) -> Self {
        Self {
            lcg: Lcg::new(base),
            parent,
            fuzzy,
        }
    }

    fn b_(&self, a: i32, b: i32) -> i32 {
        if self.lcg.next(2) == 0 {
            b
        } else {
            a
        }
    }

    fn c_(&self, a: i32, b: i32, c: i32, d: i32) -> i32 {
        if self.fuzzy {
            return [a, b, c, d][self.lcg.next(4) as usize];
        }
        if b == c && c == d {
            return b;
        }
        if a == b && a == c {
            return a;
        }
        if a == b && a == d {
            return a;
        }
        if a == c && a == d {
            return a;
        }
        if a == b && c != d {
            return a;
        }
        if a == c && b != d {
            return a;
        }
        if a == d && b != c {
            return a;
        }
        if b == c && a != d {
            return b;
        }
        if b == d && a != c {
            return b;
        }
        if c == d && a != b {
            return c;
        }
        // JE 1.7.10 selectRandom: 四值互不相同时取 a，且不消耗随机数
        a
    }
}

impl GenLayer for Zoom {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let x2 = x >> 1;
        let z2 = z >> 1;
        let w2 = (w >> 1) + 2;
        let h2 = (h >> 1) + 2;
        let src = self.parent.get(x2, z2, w2, h2);
        let tw = (w2 - 1) << 1;
        let th = (h2 - 1) << 1;
        let mut tmp = vec![0i32; tw * th];
        for j in 0..h2 - 1 {
            let mut row = (j << 1) * tw;
            let mut a = src[j * w2];
            let mut b = src[(j + 1) * w2];
            for i in 0..w2 - 1 {
                self.lcg
                    .init_chunk(((i as i32) + x2) << 1, ((j as i32) + z2) << 1);
                let c = src[i + 1 + j * w2];
                let d = src[i + 1 + (j + 1) * w2];
                tmp[row] = a;
                tmp[row + tw] = self.b_(a, b);
                row += 1;
                tmp[row] = self.b_(a, c);
                tmp[row + tw] = self.c_(a, c, b, d);
                row += 1;
                a = c;
                b = d;
            }
        }
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            let off = (j + (z & 1) as usize) * tw + (x & 1) as usize;
            out[j * w..(j + 1) * w].copy_from_slice(&tmp[off..off + w]);
        }
        out
    }
    seed_impl!();
}

// ---------- axb: AddIsland ----------
struct AddIsland {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
    edge_warm: bool,
}

impl GenLayer for AddIsland {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let a = src[i + j * sw];
                let b = src[i + 2 + j * sw];
                let c = src[i + (j + 2) * sw];
                let d = src[i + 2 + (j + 2) * sw];
                let mid = src[i + 1 + (j + 1) * sw];
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                if mid != 0 || (a == 0 && b == 0 && c == 0 && d == 0) {
                    if mid > 0 && (a == 0 || b == 0 || c == 0 || d == 0) {
                        // JE 1.7.10: nextInt(5)==0 -> forest(4)；edge_warm 时改 warm(1) 避免引入冰原
                        let r = self.lcg.next(5);
                        out[i + j * w] = if r == 0 {
                            if self.edge_warm {
                                1
                            } else {
                                4
                            }
                        } else {
                            mid
                        };
                    } else {
                        out[i + j * w] = mid;
                    }
                } else {
                    let mut n = 1;
                    let mut m = 0;
                    if a != 0 {
                        if self.lcg.next(n) == 0 {
                            m = a;
                        }
                        n += 1;
                    }
                    if b != 0 {
                        if self.lcg.next(n) == 0 {
                            m = b;
                        }
                        n += 1;
                    }
                    if c != 0 {
                        if self.lcg.next(n) == 0 {
                            m = c;
                        }
                        n += 1;
                    }
                    if d != 0 && self.lcg.next(n) == 0 {
                        m = d;
                    }
                    out[i + j * w] = if self.lcg.next(3) == 0 { m } else { 0 };
                }
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axs: RemoveTooMuchOcean ----------
struct RemoveTooMuchOcean {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for RemoveTooMuchOcean {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let u = src[i + 1 + j * sw];
                let r = src[i + 2 + (j + 1) * sw];
                let l = src[i + (j + 1) * sw];
                let d = src[i + 1 + (j + 2) * sw];
                let mid = src[i + 1 + (j + 1) * sw];
                out[i + j * w] = mid;
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                if mid == 0 && u == 0 && r == 0 && l == 0 && d == 0 && self.lcg.next(2) == 0 {
                    out[i + j * w] = 1;
                }
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axd: AddSnow（返回温度编码 1/3/4） ----------
struct AddSnow {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for AddSnow {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let v = src[i + 1 + (j + 1) * sw];
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                out[i + j * w] = if v == 0 {
                    0
                } else {
                    let r = self.lcg.next(6);
                    if SNOW_COLD_ONLY {
                        if r <= 2 {
                            3
                        } else {
                            1
                        }
                    } else if r == 0 {
                        4
                    } else if r <= 2 {
                        3
                    } else {
                        1
                    }
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- awy: 温度平滑三种模式 ----------
#[derive(Clone, Copy, PartialEq)]
enum SmoothMode {
    A,
    B,
    C,
}

struct TempSmooth {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
    mode: SmoothMode,
}

impl GenLayer for TempSmooth {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        if self.mode == SmoothMode::C {
            let src = self.parent.get(x, z, w, h);
            let mut out = vec![0i32; w * h];
            for j in 0..h {
                for i in 0..w {
                    self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                    let mut v = src[i + j * w];
                    if v != 0 && self.lcg.next(13) == 0 {
                        v |= (((1 + self.lcg.next(15)) as i32) << 8) & 3840;
                    }
                    out[i + j * w] = v;
                }
            }
            return out;
        }
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let mut v = src[i + 1 + (j + 1) * sw];
                let u = src[i + 1 + j * sw];
                let r = src[i + 2 + (j + 1) * sw];
                let l = src[i + (j + 1) * sw];
                let d = src[i + 1 + (j + 2) * sw];
                if self.mode == SmoothMode::A {
                    if v == 1 && (u == 3 || r == 3 || l == 3 || d == 3 || u == 4 || r == 4 || l == 4 || d == 4)
                    {
                        v = 2;
                    }
                } else if v == 4 && (u == 2 || r == 2 || l == 2 || d == 2 || u == 1 || r == 1 || l == 1 || d == 1)
                {
                    v = 3;
                }
                out[i + j * w] = v;
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axc: AddMushroomIsland ----------
struct AddMushroomIsland {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for AddMushroomIsland {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let a = src[i + j * sw];
                let b = src[i + 2 + j * sw];
                let c = src[i + (j + 2) * sw];
                let d = src[i + 2 + (j + 2) * sw];
                let mid = src[i + 1 + (j + 1) * sw];
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                out[i + j * w] = if mid == 0
                    && a == 0
                    && b == 0
                    && c == 0
                    && d == 0
                    && self.lcg.next(100) == 0
                {
                    MUSHROOM_ISLAND
                } else {
                    mid
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- awx: DeepOcean ----------
struct DeepOcean {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for DeepOcean {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let u = src[i + 1 + j * sw];
                let r = src[i + 2 + (j + 1) * sw];
                let l = src[i + (j + 1) * sw];
                let d = src[i + 1 + (j + 2) * sw];
                let mid = src[i + 1 + (j + 1) * sw];
                let n = [u, r, l, d].iter().filter(|&&v| v == 0).count();
                out[i + j * w] = if mid == 0 && n > 3 { DEEP_OCEAN } else { mid };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axt: RiverInit ----------
struct RiverInit {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for RiverInit {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x, z, w, h);
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                out[i + j * w] = if src[i + j * w] > 0 {
                    self.lcg.next(299999) as i32 + 2
                } else {
                    0
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axf: Biome（温度码 -> biome） ----------
const WARM: [i32; 6] = [DESERT, DESERT, DESERT, SAVANNA, SAVANNA, PLAINS];
const MEDIUM: [i32; 6] = [
    FOREST,
    ROOFED_FOREST,
    EXTREME_HILLS,
    PLAINS,
    BIRCH_FOREST,
    SWAMP,
];
const COLD: [i32; 4] = [FOREST, EXTREME_HILLS, TAIGA, PLAINS];
const ICE: [i32; 4] = [ICE_PLAINS, ICE_PLAINS, ICE_PLAINS, COLD_TAIGA];

struct Biome {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for Biome {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x, z, w, h);
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let mut v = src[i + j * w];
                let temp = (v & 3840) >> 8;
                v &= -3841;
                out[i + j * w] = if is_ocean(v) || v == MUSHROOM_ISLAND {
                    v
                } else if v == 1 {
                    if temp > 0 {
                        if self.lcg.next(3) == 0 {
                            MESA_PLATEAU
                        } else {
                            MESA_PLATEAU_F
                        }
                    } else {
                        WARM[self.lcg.next(WARM.len() as u32) as usize]
                    }
                } else if v == 2 {
                    if temp > 0 {
                        JUNGLE
                    } else {
                        MEDIUM[self.lcg.next(MEDIUM.len() as u32) as usize]
                    }
                } else if v == 3 {
                    if temp > 0 {
                        MEGA_TAIGA
                    } else {
                        COLD[self.lcg.next(COLD.len() as u32) as usize]
                    }
                } else if v == 4 {
                    ICE[self.lcg.next(ICE.len() as u32) as usize]
                } else {
                    MUSHROOM_ISLAND
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axe: BiomeEdge ----------
struct BiomeEdge {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl BiomeEdge {
    fn b_(
        &self,
        src: &[i32],
        out: &mut [i32],
        i: usize,
        j: usize,
        w: usize,
        sw: usize,
        v: i32,
        same: i32,
        edge: i32,
    ) -> bool {
        if !sim(v, same) {
            return false;
        }
        let u = src[i + 1 + j * sw];
        let r = src[i + 2 + (j + 1) * sw];
        let l = src[i + (j + 1) * sw];
        let d = src[i + 1 + (j + 2) * sw];
        out[i + j * w] = if sim(u, same) && sim(r, same) && sim(l, same) && sim(d, same) {
            v
        } else {
            edge
        };
        true
    }
}

impl GenLayer for BiomeEdge {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let v = src[i + 1 + (j + 1) * sw];
                // MCPE 0.14.3 不做 extremeHills -> extremeHillsEdge(20)：
                // 真实存档 16724 chunk 里 20 出现 0 次，而 3 占 4.89%
                if self.b_(src.as_slice(), &mut out, i, j, w, sw, v, MESA_PLATEAU_F, MESA)
                    || self.b_(src.as_slice(), &mut out, i, j, w, sw, v, MESA_PLATEAU, MESA)
                    || self.b_(src.as_slice(), &mut out, i, j, w, sw, v, MEGA_TAIGA, TAIGA)
                {
                    continue;
                }
                if v == DESERT {
                    let u = src[i + 1 + j * sw];
                    let r = src[i + 2 + (j + 1) * sw];
                    let l = src[i + (j + 1) * sw];
                    let d = src[i + 1 + (j + 2) * sw];
                    out[i + j * w] = if u != ICE_PLAINS
                        && r != ICE_PLAINS
                        && l != ICE_PLAINS
                        && d != ICE_PLAINS
                    {
                        v
                    } else {
                        EXTREME_HILLS_PLUS
                    };
                } else if v == SWAMP {
                    let u = src[i + 1 + j * sw];
                    let r = src[i + 2 + (j + 1) * sw];
                    let l = src[i + (j + 1) * sw];
                    let d = src[i + 1 + (j + 2) * sw];
                    let cold = [u, r, l, d].iter().any(|&n| {
                        n == DESERT || n == COLD_TAIGA || n == ICE_PLAINS
                    });
                    if cold {
                        out[i + j * w] = PLAINS;
                    } else if ![u, r, l, d].iter().any(|&n| n == JUNGLE) {
                        out[i + j * w] = v;
                    } else {
                        out[i + j * w] = JUNGLE_EDGE;
                    }
                } else {
                    out[i + j * w] = v;
                }
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axr: Hills ----------
struct Hills {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
    river: Arc<dyn GenLayer>,
}

/// Python 的 `(r - 2) % 29` 语义（非负）
fn mod29(v: i32) -> i32 {
    let m = (v - 2) % 29;
    if m < 0 {
        m + 29
    } else {
        m
    }
}

impl GenLayer for Hills {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let river = self.river.get(x, z, w, h);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let v = src[i + 1 + (j + 1) * sw];
                let r = river[i + j * w];
                let rare = mod29(r) == 0;
                if v != 0 && r >= 2 && mod29(r) == 1 && v < 128 {
                    out[i + j * w] = if has_biome(v + 128) { v + 128 } else { v };
                } else if self.lcg.next(3) != 0 && !rare {
                    out[i + j * w] = v;
                } else {
                    let mut nv = v;
                    if v == DESERT {
                        nv = DESERT_HILLS;
                    } else if v == FOREST {
                        nv = FOREST_HILLS;
                    } else if v == BIRCH_FOREST {
                        nv = BIRCH_FOREST_HILLS;
                    } else if v == ROOFED_FOREST {
                        nv = PLAINS;
                    } else if v == TAIGA {
                        nv = TAIGA_HILLS;
                    } else if v == MEGA_TAIGA {
                        nv = MEGA_TAIGA_HILLS;
                    } else if v == COLD_TAIGA {
                        nv = COLD_TAIGA_HILLS;
                    } else if v == PLAINS {
                        nv = if self.lcg.next(3) == 0 {
                            FOREST_HILLS
                        } else {
                            FOREST
                        };
                    } else if v == ICE_PLAINS {
                        nv = ICE_MOUNTAINS;
                    } else if v == JUNGLE {
                        nv = JUNGLE_HILLS;
                    } else if v == OCEAN {
                        nv = DEEP_OCEAN;
                    } else if v == EXTREME_HILLS {
                        nv = EXTREME_HILLS_PLUS;
                    } else if v == SAVANNA {
                        nv = SAVANNA_PLATEAU;
                    } else if sim(v, MESA_PLATEAU_F) {
                        nv = MESA;
                    } else if v == DEEP_OCEAN && self.lcg.next(3) == 0 {
                        nv = if self.lcg.next(2) == 0 { PLAINS } else { FOREST };
                    }
                    if rare && nv != v {
                        nv = if has_biome(nv + 128) { nv + 128 } else { v };
                    }
                    if nv == v {
                        out[i + j * w] = v;
                    } else {
                        let u = src[i + 1 + j * sw];
                        let rr = src[i + 2 + (j + 1) * sw];
                        let ll = src[i + (j + 1) * sw];
                        let dd = src[i + 1 + (j + 2) * sw];
                        let cnt = [u, rr, ll, dd].iter().filter(|&&n| sim(n, v)).count();
                        out[i + j * w] = if cnt >= 3 { nv } else { v };
                    }
                }
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axu: River ----------
struct River {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

fn river_c(v: i32) -> i32 {
    if v >= 2 {
        2 + (v & 1)
    } else {
        v
    }
}

impl GenLayer for River {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let l = river_c(src[i + (j + 1) * sw]);
                let r = river_c(src[i + 2 + (j + 1) * sw]);
                let u = river_c(src[i + 1 + j * sw]);
                let d = river_c(src[i + 1 + (j + 2) * sw]);
                let mid = river_c(src[i + 1 + (j + 1) * sw]);
                out[i + j * w] = if mid == l && mid == u && mid == r && mid == d {
                    -1
                } else {
                    RIVER
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axx: Smooth ----------
struct Smooth {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for Smooth {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                let l = src[i + (j + 1) * sw];
                let r = src[i + 2 + (j + 1) * sw];
                let u = src[i + 1 + j * sw];
                let d = src[i + 1 + (j + 2) * sw];
                let mut mid = src[i + 1 + (j + 1) * sw];
                if l == r && u == d {
                    self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                    mid = if self.lcg.next(2) == 0 { l } else { u };
                } else {
                    if l == r {
                        mid = l;
                    }
                    if u == d {
                        mid = u;
                    }
                }
                out[i + j * w] = mid;
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axq: RareBiome（向日葵平原） ----------
struct RareBiome {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for RareBiome {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let v = src[i + 1 + (j + 1) * sw];
                out[i + j * w] = if v == PLAINS && self.lcg.next(57) == 0 {
                    PLAINS + 128
                } else {
                    v
                };
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axw: Shore ----------
struct Shore {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl Shore {
    fn beach(
        &self,
        src: &[i32],
        out: &mut [i32],
        i: usize,
        j: usize,
        w: usize,
        sw: usize,
        v: i32,
        beach: i32,
    ) {
        if is_ocean(v) {
            out[i + j * w] = v;
            return;
        }
        let u = src[i + 1 + j * sw];
        let r = src[i + 2 + (j + 1) * sw];
        let l = src[i + (j + 1) * sw];
        let d = src[i + 1 + (j + 2) * sw];
        out[i + j * w] = if is_ocean(u) || is_ocean(r) || is_ocean(l) || is_ocean(d) {
            beach
        } else {
            v
        };
    }
}

impl GenLayer for Shore {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let src = self.parent.get(x - 1, z - 1, w + 2, h + 2);
        let sw = w + 2;
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            for i in 0..w {
                self.lcg.init_chunk(i as i32 + x, j as i32 + z);
                let v = src[i + 1 + (j + 1) * sw];
                let u = src[i + 1 + j * sw];
                let r = src[i + 2 + (j + 1) * sw];
                let l = src[i + (j + 1) * sw];
                let d = src[i + 1 + (j + 2) * sw];
                if v == MUSHROOM_ISLAND {
                    out[i + j * w] = if is_ocean(u) || is_ocean(r) || is_ocean(l) || is_ocean(d) {
                        MUSHROOM_SHORE
                    } else {
                        v
                    };
                } else if is_jungle(v) {
                    if !(is_jungle(u) && is_jungle(r) && is_jungle(l) && is_jungle(d)) {
                        out[i + j * w] = JUNGLE_EDGE;
                    } else if !(is_beach(u) && is_beach(r) && is_beach(l) && is_beach(d)) {
                        out[i + j * w] = v;
                    } else {
                        out[i + j * w] = BEACH;
                    }
                } else if v == EXTREME_HILLS || v == EXTREME_HILLS_PLUS || v == EXTREME_HILLS_EDGE {
                    self.beach(src.as_slice(), &mut out, i, j, w, sw, v, STONE_BEACH);
                } else if is_snow(v) {
                    self.beach(src.as_slice(), &mut out, i, j, w, sw, v, COLD_BEACH);
                } else if is_mesa(v) {
                    if is_beach(u) || is_beach(r) || is_beach(l) || is_beach(d) {
                        out[i + j * w] = v;
                    } else if is_mesa(u) && is_mesa(r) && is_mesa(l) && is_mesa(d) {
                        out[i + j * w] = v;
                    } else {
                        out[i + j * w] = DESERT;
                    }
                } else if v != OCEAN && v != DEEP_OCEAN && v != RIVER && v != SWAMP {
                    if !(is_beach(u) && is_beach(r) && is_beach(l) && is_beach(d)) {
                        out[i + j * w] = v;
                    } else {
                        out[i + j * w] = BEACH;
                    }
                } else {
                    out[i + j * w] = v;
                }
            }
        }
        out
    }
    seed_impl!();
}

// ---------- axv: RiverMix ----------
struct RiverMix {
    main: Arc<dyn GenLayer>,
    river: Arc<dyn GenLayer>,
}

impl GenLayer for RiverMix {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let main = self.main.get(x, z, w, h);
        let river = self.river.get(x, z, w, h);
        let mut out = vec![0i32; w * h];
        for k in 0..w * h {
            out[k] = if main[k] == OCEAN || main[k] == DEEP_OCEAN {
                main[k]
            } else if river[k] == RIVER {
                if main[k] == ICE_PLAINS {
                    FROZEN_RIVER
                } else if main[k] != MUSHROOM_ISLAND && main[k] != MUSHROOM_SHORE {
                    river[k] & 0xFF
                } else {
                    MUSHROOM_SHORE
                }
            } else {
                main[k]
            };
        }
        out
    }
    fn init_seed(&self, seed: u64) {
        self.main.init_seed(seed);
        self.river.init_seed(seed);
    }
}

// ---------- ayb: VoronoiZoom ----------
struct VoronoiZoom {
    lcg: Lcg,
    parent: Arc<dyn GenLayer>,
}

impl GenLayer for VoronoiZoom {
    fn get(&self, x: i32, z: i32, w: usize, h: usize) -> Vec<i32> {
        let x = x - 2;
        let z = z - 2;
        let x2 = x >> 2;
        let z2 = z >> 2;
        let w2 = (w >> 2) + 2;
        let h2 = (h >> 2) + 2;
        let src = self.parent.get(x2, z2, w2, h2);
        let tw = (w2 - 1) << 2;
        let th = (h2 - 1) << 2;
        let mut tmp = vec![0i32; tw * th];
        for j in 0..h2 - 1 {
            let off = (j << 2) * tw;
            let mut a = src[j * w2];
            let mut b = src[(j + 1) * w2];
            for i in 0..w2 - 1 {
                self.lcg
                    .init_chunk(((i as i32) + x2) << 2, ((j as i32) + z2) << 2);
                let d1 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6;
                let d2 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6;
                self.lcg
                    .init_chunk(((i as i32) + x2 + 1) << 2, ((j as i32) + z2) << 2);
                let d3 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6 + 4.0;
                let d4 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6;
                self.lcg
                    .init_chunk(((i as i32) + x2) << 2, ((j as i32) + z2 + 1) << 2);
                let d5 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6;
                let d6 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6 + 4.0;
                self.lcg
                    .init_chunk(((i as i32) + x2 + 1) << 2, ((j as i32) + z2 + 1) << 2);
                let d7 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6 + 4.0;
                let d8 = (self.lcg.next(1024) as f64 / 1024.0 - 0.5) * 3.6 + 4.0;
                let c = src[i + 1 + j * w2] & 0xFF;
                let e = src[i + 1 + (j + 1) * w2] & 0xFF;
                for dj in 0..4usize {
                    let row = off + dj * tw + (i << 2);
                    for di in 0..4usize {
                        let q1 = (di as f64 - d1).powi(2) + (dj as f64 - d2).powi(2);
                        let q2 = (di as f64 - d3).powi(2) + (dj as f64 - d4).powi(2);
                        let q3 = (di as f64 - d5).powi(2) + (dj as f64 - d6).powi(2);
                        let q4 = (di as f64 - d7).powi(2) + (dj as f64 - d8).powi(2);
                        tmp[row + di] = if q1 < q2 && q1 < q3 && q1 < q4 {
                            a
                        } else if q2 < q1 && q2 < q3 && q2 < q4 {
                            c
                        } else if q3 < q1 && q3 < q2 && q3 < q4 {
                            b
                        } else {
                            e
                        };
                    }
                }
                a = c;
                b = e;
            }
        }
        let mut out = vec![0i32; w * h];
        for j in 0..h {
            let off = (j + ((z & 3) as usize)) * tw + ((x & 3) as usize);
            out[j * w..(j + 1) * w].copy_from_slice(&tmp[off..off + w]);
        }
        out
    }
    seed_impl!();
}

// ---------- axn.a: 链构建（build_old + 实测配置） ----------
/// BIOME_ZOOM=0、RIVER_ZOOMS=5、TAIL_ZOOMS=3
fn build_chain() -> Arc<dyn GenLayer> {
    let mut l: Arc<dyn GenLayer> = Arc::new(Island { lcg: Lcg::new(1) });
    l = Arc::new(Zoom::new(2000, l, true)); // FuzzyZoom
    l = Arc::new(AddIsland {
        lcg: Lcg::new(1),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(Zoom::new(2001, l, false));
    l = Arc::new(AddIsland {
        lcg: Lcg::new(2),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(AddIsland {
        lcg: Lcg::new(50),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(AddIsland {
        lcg: Lcg::new(70),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(RemoveTooMuchOcean {
        lcg: Lcg::new(2),
        parent: l,
    });
    l = Arc::new(AddSnow {
        lcg: Lcg::new(2),
        parent: l,
    });
    l = Arc::new(AddIsland {
        lcg: Lcg::new(3),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(TempSmooth {
        lcg: Lcg::new(2),
        parent: l,
        mode: SmoothMode::A,
    });
    l = Arc::new(TempSmooth {
        lcg: Lcg::new(2),
        parent: l,
        mode: SmoothMode::B,
    });
    l = Arc::new(TempSmooth {
        lcg: Lcg::new(3),
        parent: l,
        mode: SmoothMode::C,
    });
    l = Arc::new(Zoom::new(2002, l, false));
    l = Arc::new(Zoom::new(2003, l, false));
    l = Arc::new(AddIsland {
        lcg: Lcg::new(4),
        parent: l,
        edge_warm: false,
    });
    l = Arc::new(AddMushroomIsland {
        lcg: Lcg::new(5),
        parent: l,
    });
    l = Arc::new(DeepOcean {
        lcg: Lcg::new(4),
        parent: l,
    });
    // BIOME_ZOOM=0：此处不 Zoom

    // 支线：RiverInit -> Zoom x5 -> River -> Smooth
    let river0: Arc<dyn GenLayer> = Arc::new(RiverInit {
        lcg: Lcg::new(100),
        parent: l.clone(),
    });
    // Hills 的第三参数 = Zoom(1000, Zoom(1000, RiverInit))
    let mut shore: Arc<dyn GenLayer> = river0.clone();
    shore = Arc::new(Zoom::new(1000, shore, false));
    shore = Arc::new(Zoom::new(1000, shore, false));

    // 主干：Biome -> Zoom x2 -> BiomeEdge -> Hills
    let mut main: Arc<dyn GenLayer> = Arc::new(Biome {
        lcg: Lcg::new(200),
        parent: l.clone(),
    });
    main = Arc::new(Zoom::new(1000, main, false));
    main = Arc::new(Zoom::new(1001, main, false));
    main = Arc::new(BiomeEdge {
        lcg: Lcg::new(1000),
        parent: main,
    });
    main = Arc::new(Hills {
        lcg: Lcg::new(1000),
        parent: main,
        river: shore,
    });

    let mut river: Arc<dyn GenLayer> = river0;
    for id in [1000i64, 1000, 1001, 1002, 1003] {
        river = Arc::new(Zoom::new(id, river, false));
    }
    river = Arc::new(River {
        lcg: Lcg::new(1),
        parent: river,
    });
    river = Arc::new(Smooth {
        lcg: Lcg::new(1000),
        parent: river,
    });

    main = Arc::new(RareBiome {
        lcg: Lcg::new(1001),
        parent: main,
    });
    for k in 0..3 {
        main = Arc::new(Zoom::new(1000 + k, main, false));
    }
    main = Arc::new(Smooth {
        lcg: Lcg::new(1000),
        parent: main,
    });
    main = Arc::new(RiverMix {
        main,
        river,
    });
    main = Arc::new(VoronoiZoom {
        lcg: Lcg::new(10),
        parent: main,
    });
    // TAIL=after：Voronoi 之后再扩张一次并补岸
    main = Arc::new(AddIsland {
        lcg: Lcg::new(3),
        parent: main,
        edge_warm: false,
    });
    main = Arc::new(Shore {
        lcg: Lcg::new(1000),
        parent: main,
    });
    main
}

/// biome 生成器。内部链只在构造时建一次，`chunk_biomes` 可反复调用。
#[derive(Clone)]
pub struct BiomeGen {
    chain: Arc<dyn GenLayer>,
    cache: RefCell<HashMap<(i32, i32), Arc<[u8; 256]>>>,
}

impl BiomeGen {
    pub fn new(seed: i64) -> Self {
        let chain = build_chain();
        chain.init_seed(seed as u64);
        Self {
            chain,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// 16x16 个 block 的 biome id，索引为 x + z*16（带 chunk 级缓存）
    pub fn chunk_biomes(&self, cx: i32, cz: i32) -> Arc<[u8; 256]> {
        if let Some(v) = self.cache.borrow().get(&(cx, cz)) {
            return v.clone();
        }
        let v: Arc<[u8; 256]> = Arc::new(self.compute_chunk(cx, cz));
        self.cache.borrow_mut().insert((cx, cz), v.clone());
        v
    }

    /// 最终层是 1 cell = 1 方块（run-length 实测：预测均值 15 cell、真实 10.6 方块），
    /// 所以一个区块直接取 16x16 cell，不做降采样。
    fn compute_chunk(&self, cx: i32, cz: i32) -> [u8; 256] {
        const N: usize = 16;
        let src = self.chain.get(cx * N as i32, cz * N as i32, N, N);
        let mut out = [0u8; 256];
        for i in 0..N * N {
            out[i] = (src[i] & 0xFF) as u8;
        }
        out
    }

    /// 单个 block 的 biome
    pub fn biome_at(&self, x: i32, z: i32) -> u8 {
        self.chunk_biomes(x >> 4, z >> 4)[(x & 15) as usize + (z & 15) as usize * 16]
    }
}
