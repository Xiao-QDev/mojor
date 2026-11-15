//! MCPE 0.14.3 biome 生成 —— `BiomeSource` → `Layer::getDefaultLayers`。
//!
//! x86 版 `libminecraftpe.so` 的 `Layer::getDefaultLayers` 反汇编（层的创建顺序 + 每层 seed）
//! 与 JE 1.7.10 `GenLayer.initializeAllBiomeGenerators` 逐层一致，层内随机数也是同一个
//! Java LCG（`initRandom` = `initChunkSeed`，`nextRandom` = `(s>>24)%n`）。
//! 因此这里**逐行照搬 JE 1.7.10 的语义**，不做任何统计调参：
//!
//! ```text
//! Island(1) → FuzzyZoom(2000) → AddIsland(1) → Zoom(2001) → AddIsland(2,50,70)
//!   → RemoveTooMuchOcean(2) → AddSnow(2) → AddIsland(3)
//!   → Edge(2,CoolWarm) → Edge(2,HeatIce) → Edge(3,Special)
//!   → Zoom(2002,2003) → AddIsland(4) → AddMushroomIsland(5) → DeepOcean(4)   = base
//! river:  RiverInit(100, base) ─┬→ magnify(1000,2) ──────────────────┐ (Hills 的第二输入)
//!                               └→ magnify(1000,2) → magnify(1000,4) → River(1) → Smooth(1000)
//! biome:  Biome(200, base) → magnify(1000,2) → BiomeEdge(1000) → Hills(1000, ·, river)
//!   → RareBiome(1001) → [Zoom(1000+i); i==0: AddIsland(3); i==1: Shore(1000)] × 4
//!   → Smooth(1000)
//! RiverMix(100, biome, river)  —— 1:4 分辨率，`fillRawBiomeData` / 地形密度用
//! VoronoiZoom(10, RiverMix)     —— 1:1 分辨率，`fillBiomeData` / 每方块 biome 用
//! ```
//!
//! 之前版本里为了“拟合存档统计”改过的若干处（AddSnow 冷带概率、AddIsland 侵蚀输出、
//! Zoom 四值互异时不取随机、去掉 +128 变种、尾部额外的 AddIsland/Shore 等）全部回到
//! 1.7.10 原义：`realistic_terrain/biome/layers`（独立的 Java→PHP→Rust 移植）也印证了这些写法。
//!
//! 实现上每次 `get` 用局部随机状态（Java 里 chunkSeed 是成员，但父层总是先整块算完，
//! 两者等价），所以整条链是只读的、`Send + Sync`，可以随意共享。

use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// ---------- biome ids（JE 1.7.10 / MCPE 0.14 相同） ----------
const OCEAN: i32 = 0;
const PLAINS: i32 = 1;
const DESERT: i32 = 2;
const EXTREME_HILLS: i32 = 3;
const FOREST: i32 = 4;
const TAIGA: i32 = 5;
const SWAMP: i32 = 6;
const RIVER: i32 = 7;
const FROZEN_OCEAN: i32 = 10;
const FROZEN_RIVER: i32 = 11;
const ICE_PLAINS: i32 = 12;
const ICE_MOUNTAINS: i32 = 13;
const MUSHROOM_ISLAND: i32 = 14;
const MUSHROOM_SHORE: i32 = 15;
const BEACH: i32 = 16;
const DESERT_HILLS: i32 = 17;
const FOREST_HILLS: i32 = 18;
const TAIGA_HILLS: i32 = 19;
const EXTREME_HILLS_EDGE: i32 = 20;
const JUNGLE: i32 = 21;
const JUNGLE_HILLS: i32 = 22;
const JUNGLE_EDGE: i32 = 23;
const DEEP_OCEAN: i32 = 24;
const STONE_BEACH: i32 = 25;
const COLD_BEACH: i32 = 26;
const BIRCH_FOREST: i32 = 27;
const BIRCH_FOREST_HILLS: i32 = 28;
const ROOFED_FOREST: i32 = 29;
const COLD_TAIGA: i32 = 30;
const COLD_TAIGA_HILLS: i32 = 31;
const MEGA_TAIGA: i32 = 32;
const MEGA_TAIGA_HILLS: i32 = 33;
const EXTREME_HILLS_PLUS: i32 = 34;
const SAVANNA: i32 = 35;
const SAVANNA_PLATEAU: i32 = 36;
const MESA: i32 = 37;
const MESA_PLATEAU_F: i32 = 38;
const MESA_PLATEAU: i32 = 39;

/// `BiomeGenBase.getBiomeClass()`：同类 biome 在 `compareBiomesById` 里视为相等。
/// 变种（id+128）的 class 等于其基础 biome 的 class。`None` = 未注册的 id。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Ocean,
    Plains,
    Desert,
    Hills,
    Forest,
    Taiga,
    Swamp,
    River,
    Hell,
    End,
    Snow,
    Mushroom,
    Beach,
    Jungle,
    StoneBeach,
    Savanna,
    Mesa,
}

/// 1.7.10 注册了变种的基础 biome（`getBiome(id + 128) != null`）
fn has_mutation(id: i32) -> bool {
    matches!(
        id,
        PLAINS
            | DESERT
            | EXTREME_HILLS
            | FOREST
            | TAIGA
            | SWAMP
            | ICE_PLAINS
            | JUNGLE
            | JUNGLE_EDGE
            | BIRCH_FOREST
            | BIRCH_FOREST_HILLS
            | ROOFED_FOREST
            | COLD_TAIGA
            | MEGA_TAIGA
            | MEGA_TAIGA_HILLS
            | EXTREME_HILLS_PLUS
            | SAVANNA
            | SAVANNA_PLATEAU
            | MESA
            | MESA_PLATEAU_F
            | MESA_PLATEAU
    )
}

fn exists(id: i32) -> bool {
    class_of(id).is_some()
}

fn class_of(id: i32) -> Option<Class> {
    let base = if (128..256).contains(&id) {
        if !has_mutation(id - 128) {
            return None;
        }
        id - 128
    } else {
        id
    };
    Some(match base {
        OCEAN | FROZEN_OCEAN | DEEP_OCEAN => Class::Ocean,
        PLAINS => Class::Plains,
        DESERT | DESERT_HILLS => Class::Desert,
        EXTREME_HILLS | EXTREME_HILLS_EDGE | EXTREME_HILLS_PLUS => Class::Hills,
        FOREST | FOREST_HILLS | BIRCH_FOREST | BIRCH_FOREST_HILLS | ROOFED_FOREST => Class::Forest,
        TAIGA | TAIGA_HILLS | COLD_TAIGA | COLD_TAIGA_HILLS | MEGA_TAIGA | MEGA_TAIGA_HILLS => {
            Class::Taiga
        }
        SWAMP => Class::Swamp,
        RIVER | FROZEN_RIVER => Class::River,
        8 => Class::Hell,
        9 => Class::End,
        ICE_PLAINS | ICE_MOUNTAINS => Class::Snow,
        MUSHROOM_ISLAND | MUSHROOM_SHORE => Class::Mushroom,
        BEACH | COLD_BEACH => Class::Beach,
        JUNGLE | JUNGLE_HILLS | JUNGLE_EDGE => Class::Jungle,
        STONE_BEACH => Class::StoneBeach,
        SAVANNA | SAVANNA_PLATEAU => Class::Savanna,
        MESA | MESA_PLATEAU_F | MESA_PLATEAU => Class::Mesa,
        _ => return None,
    })
}

/// `BiomeGenBase.temperature`（变种按基础 biome 取，只用于 TempCategory 粗分）
fn temperature(id: i32) -> f32 {
    let id = if id >= 128 { id - 128 } else { id };
    match id {
        OCEAN | RIVER | 9 | DEEP_OCEAN => 0.5,
        PLAINS | SWAMP | BEACH => 0.8,
        DESERT | 8 | DESERT_HILLS | MESA | MESA_PLATEAU_F | MESA_PLATEAU => 2.0,
        EXTREME_HILLS | EXTREME_HILLS_EDGE | STONE_BEACH | EXTREME_HILLS_PLUS => 0.2,
        FOREST | FOREST_HILLS | ROOFED_FOREST => 0.7,
        TAIGA | TAIGA_HILLS => 0.25,
        FROZEN_OCEAN | FROZEN_RIVER | ICE_PLAINS | ICE_MOUNTAINS => 0.0,
        MUSHROOM_ISLAND | MUSHROOM_SHORE => 0.9,
        JUNGLE | JUNGLE_HILLS | JUNGLE_EDGE => 0.95,
        COLD_BEACH => 0.05,
        BIRCH_FOREST | BIRCH_FOREST_HILLS => 0.6,
        COLD_TAIGA | COLD_TAIGA_HILLS => -0.5,
        MEGA_TAIGA | MEGA_TAIGA_HILLS => 0.3,
        SAVANNA => 1.2,
        SAVANNA_PLATEAU => 1.0,
        _ => 0.5,
    }
}

#[derive(PartialEq, Eq)]
enum TempCategory {
    Ocean,
    Cold,
    Medium,
    Warm,
}

fn temp_category(id: i32) -> TempCategory {
    if class_of(id) == Some(Class::Ocean) {
        return TempCategory::Ocean;
    }
    let t = temperature(id);
    if t < 0.2 {
        TempCategory::Cold
    } else if t < 1.0 {
        TempCategory::Medium
    } else {
        TempCategory::Warm
    }
}

/// `BiomeGenBase.enableSnow`（变种继承基础 biome）
fn is_snowy(id: i32) -> bool {
    matches!(
        id,
        FROZEN_OCEAN
            | FROZEN_RIVER
            | ICE_PLAINS
            | ICE_MOUNTAINS
            | COLD_BEACH
            | COLD_TAIGA
            | COLD_TAIGA_HILLS
    ) || id == ICE_PLAINS + 128
        || id == COLD_TAIGA + 128
}

/// `GenLayer.isBiomeOceanic`
fn is_oceanic(id: i32) -> bool {
    id == OCEAN || id == DEEP_OCEAN || id == FROZEN_OCEAN
}

fn is_mesa(id: i32) -> bool {
    class_of(id) == Some(Class::Mesa)
}

/// `GenLayer.compareBiomesById`
fn same_biome(a: i32, b: i32) -> bool {
    if a == b {
        return true;
    }
    if a == MESA_PLATEAU_F || a == MESA_PLATEAU {
        return b == MESA_PLATEAU_F || b == MESA_PLATEAU;
    }
    match (class_of(a), class_of(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

// ---------- Java LCG（GenLayer 的 baseSeed / worldGenSeed / chunkSeed） ----------
const A: i64 = 6364136223846793005;
const B: i64 = 1442695040888963407;

#[inline]
fn step(s: i64, salt: i64) -> i64 {
    s.wrapping_mul(s.wrapping_mul(A).wrapping_add(B)).wrapping_add(salt)
}

/// `new GenLayer(seed)` 里的 baseSeed
fn base_seed(seed: i64) -> i64 {
    let mut s = seed;
    for _ in 0..3 {
        s = step(s, seed);
    }
    s
}

/// `initWorldGenSeed` 的本层部分
fn world_seed(world: i64, base: i64) -> i64 {
    let mut s = world;
    for _ in 0..3 {
        s = step(s, base);
    }
    s
}

/// 一层的种子：`base` 构造时定，`world` 由 `init_world_seed` 写入（未播种时为 0）
#[derive(Clone, Copy)]
struct Seed {
    base: i64,
    world: i64,
}

impl Seed {
    fn new(seed: i64) -> Self {
        Self {
            base: base_seed(seed),
            world: 0,
        }
    }

    fn init(&mut self, world: i64) {
        self.world = world_seed(world, self.base);
    }

    /// `initChunkSeed(x, z)`
    #[inline]
    fn chunk(&self, x: i32, z: i32) -> Rng {
        let (x, z) = (x as i64, z as i64);
        let mut cs = self.world;
        cs = step(cs, x);
        cs = step(cs, z);
        cs = step(cs, x);
        cs = step(cs, z);
        Rng { cs, ws: self.world }
    }
}

struct Rng {
    cs: i64,
    ws: i64,
}

impl Rng {
    /// `nextInt(n)`
    #[inline]
    fn next(&mut self, n: i32) -> i32 {
        let mut r = ((self.cs >> 24) % n as i64) as i32;
        if r < 0 {
            r += n;
        }
        self.cs = step(self.cs, self.ws);
        r
    }

    fn pick(&mut self, v: &[i32]) -> i32 {
        v[self.next(v.len() as i32) as usize]
    }

    /// `GenLayerZoom.selectModeOrRandom`
    fn mode_or_random(&mut self, a: i32, b: i32, c: i32, d: i32) -> i32 {
        if b == c && c == d {
            b
        } else if a == b && a == c {
            a
        } else if a == b && a == d {
            a
        } else if a == c && a == d {
            a
        } else if a == b && c != d {
            a
        } else if a == c && b != d {
            a
        } else if a == d && b != c {
            a
        } else if b == c && a != d {
            b
        } else if b == d && a != c {
            b
        } else if c == d && a != b {
            c
        } else {
            self.pick(&[a, b, c, d])
        }
    }
}

// ---------- 层 ----------

pub trait GenLayer: Send + Sync {
    /// `getInts(x, z, w, h)`，输出索引 `[dx + dz * w]`
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32>;
    /// `initWorldGenSeed`：先父层、再本层
    fn init_world_seed(&mut self, world: i64);
}

type Layer = Box<dyn GenLayer>;

/// 读 3×3 邻域的辅助：`p` 是 `(w+2)×(h+2)` 的父层输出
struct Grid<'a> {
    p: &'a [i32],
    pw: usize,
}

impl Grid<'_> {
    #[inline]
    fn at(&self, j: i32, i: i32) -> i32 {
        self.p[j as usize + i as usize * self.pw]
    }
    /// (中心, 北, 东, 西, 南)
    #[inline]
    fn cross(&self, j: i32, i: i32) -> (i32, i32, i32, i32, i32) {
        (
            self.at(j + 1, i + 1),
            self.at(j + 1, i),
            self.at(j + 2, i + 1),
            self.at(j, i + 1),
            self.at(j + 1, i + 2),
        )
    }
    /// (中心, 西北, 东北, 西南, 东南)
    #[inline]
    fn diag(&self, j: i32, i: i32) -> (i32, i32, i32, i32, i32) {
        (
            self.at(j + 1, i + 1),
            self.at(j, i),
            self.at(j + 2, i),
            self.at(j, i + 2),
            self.at(j + 2, i + 2),
        )
    }
}

/// 对一个带 1 格边框的父层逐格计算
fn map3x3(
    parent: &Layer,
    x: i32,
    z: i32,
    w: i32,
    h: i32,
    mut f: impl FnMut(&Grid, i32, i32) -> i32,
) -> Vec<i32> {
    let p = parent.get(x - 1, z - 1, w + 2, h + 2);
    let g = Grid {
        p: &p,
        pw: (w + 2) as usize,
    };
    let mut out = vec![0; (w * h) as usize];
    for i in 0..h {
        for j in 0..w {
            out[(j + i * w) as usize] = f(&g, j, i);
        }
    }
    out
}

macro_rules! single_parent_seed {
    () => {
        fn init_world_seed(&mut self, world: i64) {
            self.parent.init_world_seed(world);
            self.seed.init(world);
        }
    };
}

// --- GenLayerIsland ---
struct Island {
    seed: Seed,
}

impl GenLayer for Island {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let mut out = vec![0; (w * h) as usize];
        for i in 0..h {
            for j in 0..w {
                let mut r = self.seed.chunk(x + j, z + i);
                out[(j + i * w) as usize] = if r.next(10) == 0 { 1 } else { 0 };
            }
        }
        if x > -w && x <= 0 && z > -h && z <= 0 {
            out[(-x - z * w) as usize] = 1;
        }
        out
    }
    fn init_world_seed(&mut self, world: i64) {
        self.seed.init(world);
    }
}

// --- GenLayerZoom / GenLayerFuzzyZoom ---
struct Zoom {
    seed: Seed,
    parent: Layer,
    fuzzy: bool,
}

impl GenLayer for Zoom {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let px = x >> 1;
        let pz = z >> 1;
        let pw = (w >> 1) + 2;
        let ph = (h >> 1) + 2;
        let p = self.parent.get(px, pz, pw, ph);
        let tw = ((pw - 1) << 1) as usize;
        let th = ((ph - 1) << 1) as usize;
        let mut tmp = vec![0; tw * th];
        for k in 0..ph - 1 {
            let mut idx = ((k << 1) as usize) * tw;
            let mut a = p[(k * pw) as usize];
            let mut c = p[((k + 1) * pw) as usize];
            for i in 0..pw - 1 {
                let mut r = self.seed.chunk((i + px) << 1, (k + pz) << 1);
                let b = p[(i + 1 + k * pw) as usize];
                let d = p[(i + 1 + (k + 1) * pw) as usize];
                tmp[idx] = a;
                tmp[idx + tw] = r.pick(&[a, c]);
                idx += 1;
                tmp[idx] = r.pick(&[a, b]);
                tmp[idx + tw] = if self.fuzzy {
                    r.pick(&[a, b, c, d])
                } else {
                    r.mode_or_random(a, b, c, d)
                };
                idx += 1;
                a = b;
                c = d;
            }
        }
        let mut out = vec![0; (w * h) as usize];
        for k in 0..h {
            let src = ((k + (z & 1)) as usize) * tw + (x & 1) as usize;
            out[(k * w) as usize..((k + 1) * w) as usize].copy_from_slice(&tmp[src..src + w as usize]);
        }
        out
    }
    single_parent_seed!();
}

fn zoom(seed: i64, parent: Layer) -> Layer {
    Box::new(Zoom {
        seed: Seed::new(seed),
        parent,
        fuzzy: false,
    })
}

/// `GenLayerZoom.magnify`
fn magnify(seed: i64, mut layer: Layer, n: i32) -> Layer {
    for i in 0..n {
        layer = zoom(seed + i as i64, layer);
    }
    layer
}

// --- GenLayerAddIsland ---
struct AddIsland {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for AddIsland {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, a, b, c, d) = g.diag(j, i);
            let mut r = self.seed.chunk(j + x, i + z);
            if v != 0 || (a == 0 && b == 0 && c == 0 && d == 0) {
                if v > 0 && (a == 0 || b == 0 || c == 0 || d == 0) {
                    if r.next(5) == 0 {
                        if v == 4 {
                            4
                        } else {
                            0
                        }
                    } else {
                        v
                    }
                } else {
                    v
                }
            } else {
                let mut n = 1;
                let mut pick = 1;
                for nb in [a, b, c, d] {
                    if nb != 0 {
                        let hit = r.next(n) == 0;
                        n += 1;
                        if hit {
                            pick = nb;
                        }
                    }
                }
                if r.next(3) == 0 {
                    pick
                } else if pick == 4 {
                    4
                } else {
                    0
                }
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerRemoveTooMuchOcean ---
struct RemoveTooMuchOcean {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for RemoveTooMuchOcean {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, n, e, wst, s) = g.cross(j, i);
            let mut r = self.seed.chunk(j + x, i + z);
            if v == 0 && n == 0 && e == 0 && wst == 0 && s == 0 && r.next(2) == 0 {
                1
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerAddSnow ---
struct AddSnow {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for AddSnow {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let v = g.at(j + 1, i + 1);
            let mut r = self.seed.chunk(j + x, i + z);
            if v == 0 {
                0
            } else {
                match r.next(6) {
                    0 => 4,
                    1 => 3,
                    _ => 1,
                }
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerEdge（MCPE: AddEdgeLayer）---
#[derive(Clone, Copy, PartialEq, Eq)]
enum EdgeMode {
    CoolWarm,
    HeatIce,
    Special,
}

struct Edge {
    seed: Seed,
    parent: Layer,
    mode: EdgeMode,
}

impl GenLayer for Edge {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        match self.mode {
            EdgeMode::CoolWarm => map3x3(&self.parent, x, z, w, h, |g, j, i| {
                let (v, n, e, wst, s) = g.cross(j, i);
                if v == 1 {
                    let nb = [n, e, wst, s];
                    if nb.iter().any(|&t| t == 3 || t == 4) {
                        return 2;
                    }
                }
                v
            }),
            EdgeMode::HeatIce => map3x3(&self.parent, x, z, w, h, |g, j, i| {
                let (v, n, e, wst, s) = g.cross(j, i);
                if v == 4 {
                    let nb = [n, e, wst, s];
                    if nb.iter().any(|&t| t == 1 || t == 2) {
                        return 3;
                    }
                }
                v
            }),
            EdgeMode::Special => {
                let p = self.parent.get(x, z, w, h);
                let mut out = vec![0; (w * h) as usize];
                for i in 0..h {
                    for j in 0..w {
                        let mut r = self.seed.chunk(j + x, i + z);
                        let mut v = p[(j + i * w) as usize];
                        if v != 0 && r.next(13) == 0 {
                            v |= ((1 + r.next(15)) << 8) & 0xF00;
                        }
                        out[(j + i * w) as usize] = v;
                    }
                }
                out
            }
        }
    }
    single_parent_seed!();
}

// --- GenLayerAddMushroomIsland ---
struct AddMushroomIsland {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for AddMushroomIsland {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, a, b, c, d) = g.diag(j, i);
            let mut r = self.seed.chunk(j + x, i + z);
            if v == 0 && a == 0 && b == 0 && c == 0 && d == 0 && r.next(100) == 0 {
                MUSHROOM_ISLAND
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerDeepOcean ---
struct DeepOcean {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for DeepOcean {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, n, e, wst, s) = g.cross(j, i);
            let oceans = [n, e, wst, s].iter().filter(|&&t| t == 0).count();
            if v == 0 && oceans > 3 {
                DEEP_OCEAN
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerRiverInit ---
struct RiverInit {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for RiverInit {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let p = self.parent.get(x, z, w, h);
        let mut out = vec![0; (w * h) as usize];
        for i in 0..h {
            for j in 0..w {
                let mut r = self.seed.chunk(j + x, i + z);
                let k = (j + i * w) as usize;
                out[k] = if p[k] > 0 { r.next(299999) + 2 } else { 0 };
            }
        }
        out
    }
    single_parent_seed!();
}

// --- GenLayerBiome（MCPE: BiomeInitLayer）---
const WARM: [i32; 6] = [DESERT, DESERT, DESERT, SAVANNA, SAVANNA, PLAINS];
const MEDIUM: [i32; 6] = [FOREST, ROOFED_FOREST, EXTREME_HILLS, PLAINS, BIRCH_FOREST, SWAMP];
const COLD: [i32; 4] = [FOREST, EXTREME_HILLS, TAIGA, PLAINS];
const ICE: [i32; 4] = [ICE_PLAINS, ICE_PLAINS, ICE_PLAINS, COLD_TAIGA];

struct BiomeInit {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for BiomeInit {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let p = self.parent.get(x, z, w, h);
        let mut out = vec![0; (w * h) as usize];
        for i in 0..h {
            for j in 0..w {
                let mut r = self.seed.chunk(j + x, i + z);
                let raw = p[(j + i * w) as usize];
                let special = (raw & 0xF00) >> 8;
                let v = raw & !0xF00;
                out[(j + i * w) as usize] = if is_oceanic(v) || v == MUSHROOM_ISLAND {
                    v
                } else if v == 1 {
                    if special > 0 {
                        if r.next(3) == 0 {
                            MESA_PLATEAU
                        } else {
                            MESA_PLATEAU_F
                        }
                    } else {
                        r.pick(&WARM)
                    }
                } else if v == 2 {
                    if special > 0 {
                        JUNGLE
                    } else {
                        r.pick(&MEDIUM)
                    }
                } else if v == 3 {
                    if special > 0 {
                        MEGA_TAIGA
                    } else {
                        r.pick(&COLD)
                    }
                } else if v == 4 {
                    r.pick(&ICE)
                } else {
                    MUSHROOM_ISLAND
                };
            }
        }
        out
    }
    single_parent_seed!();
}

// --- GenLayerBiomeEdge ---
struct BiomeEdge {
    seed: Seed,
    parent: Layer,
}

/// `GenLayerBiomeEdge.func_151634_b`
fn can_be_neighbors(a: i32, b: i32) -> bool {
    if same_biome(a, b) {
        return true;
    }
    if !exists(a) || !exists(b) {
        return false;
    }
    let ta = temp_category(a);
    let tb = temp_category(b);
    ta == tb || ta == TempCategory::Medium || tb == TempCategory::Medium
}

impl GenLayer for BiomeEdge {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, n, e, wst, s) = g.cross(j, i);
            let nb = [n, e, wst, s];
            // replaceBiomeEdgeIfNecessary(extremeHills → extremeHillsEdge)
            if same_biome(v, EXTREME_HILLS) {
                return if nb.iter().all(|&t| can_be_neighbors(t, v)) {
                    v
                } else {
                    EXTREME_HILLS_EDGE
                };
            }
            // replaceBiomeEdge（严格 id 相等）
            for (target, edge) in [
                (MESA_PLATEAU_F, MESA),
                (MESA_PLATEAU, MESA),
                (MEGA_TAIGA, TAIGA),
            ] {
                if v == target {
                    return if nb.iter().all(|&t| same_biome(t, v)) {
                        v
                    } else {
                        edge
                    };
                }
            }
            if v == DESERT {
                if nb.iter().all(|&t| t != ICE_PLAINS) {
                    v
                } else {
                    EXTREME_HILLS_PLUS
                }
            } else if v == SWAMP {
                if nb
                    .iter()
                    .all(|&t| t != DESERT && t != COLD_TAIGA && t != ICE_PLAINS)
                {
                    if nb.iter().all(|&t| t != JUNGLE) {
                        v
                    } else {
                        JUNGLE_EDGE
                    }
                } else {
                    PLAINS
                }
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

/// 1.7.10 的 `GenLayerHills` 没有覆写 `initWorldGenSeed`，第二输入（river 支路）
/// 不随 Hills 播种：其上的两层 Zoom 保持 worldGenSeed=0，下面共享的
/// `RiverInit` 及基础树则因为被主 river 支路共享而已经播过种。
const HILLS_RIVER_SEEDED: bool = false;

// --- GenLayerHills（MCPE: RegionHillsLayer）---
struct Hills {
    seed: Seed,
    parent: Layer,
    river: Layer,
}

impl GenLayer for Hills {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let rv = self.river.get(x - 1, z - 1, w + 2, h + 2);
        let pw = (w + 2) as usize;
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let mut r = self.seed.chunk(j + x, i + z);
            let v = g.at(j + 1, i + 1);
            let l = rv[(j + 1) as usize + (i + 1) as usize * pw];
            let flag = (l - 2) % 29 == 0;
            if v != 0 && l >= 2 && (l - 2) % 29 == 1 && v < 128 {
                return if exists(v + 128) { v + 128 } else { v };
            }
            if r.next(3) != 0 && !flag {
                return v;
            }
            let mut t = v;
            if v == DESERT {
                t = DESERT_HILLS;
            } else if v == FOREST {
                t = FOREST_HILLS;
            } else if v == BIRCH_FOREST {
                t = BIRCH_FOREST_HILLS;
            } else if v == ROOFED_FOREST {
                t = PLAINS;
            } else if v == TAIGA {
                t = TAIGA_HILLS;
            } else if v == MEGA_TAIGA {
                t = MEGA_TAIGA_HILLS;
            } else if v == COLD_TAIGA {
                t = COLD_TAIGA_HILLS;
            } else if v == PLAINS {
                t = if r.next(3) == 0 { FOREST_HILLS } else { FOREST };
            } else if v == ICE_PLAINS {
                t = ICE_MOUNTAINS;
            } else if v == JUNGLE {
                t = JUNGLE_HILLS;
            } else if v == OCEAN {
                t = DEEP_OCEAN;
            } else if v == EXTREME_HILLS {
                t = EXTREME_HILLS_PLUS;
            } else if v == SAVANNA {
                t = SAVANNA_PLATEAU;
            } else if same_biome(v, MESA_PLATEAU_F) {
                t = MESA;
            } else if v == DEEP_OCEAN && r.next(3) == 0 {
                t = if r.next(2) == 0 { PLAINS } else { FOREST };
            }
            if flag && t != v {
                t = if exists(t + 128) { t + 128 } else { v };
            }
            if t == v {
                return v;
            }
            let (_, n, e, wst, s) = g.cross(j, i);
            let same = [n, e, wst, s].iter().filter(|&&q| same_biome(q, v)).count();
            if same >= 3 {
                t
            } else {
                v
            }
        })
    }
    fn init_world_seed(&mut self, world: i64) {
        self.parent.init_world_seed(world);
        if HILLS_RIVER_SEEDED {
            self.river.init_world_seed(world);
        }
        self.seed.init(world);
    }
}

// --- GenLayerRiver ---
struct River {
    seed: Seed,
    parent: Layer,
}

fn river_filter(v: i32) -> i32 {
    if v >= 2 {
        2 + (v & 1)
    } else {
        v
    }
}

impl GenLayer for River {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, n, e, wst, s) = g.cross(j, i);
            let v = river_filter(v);
            if v == river_filter(wst)
                && v == river_filter(n)
                && v == river_filter(e)
                && v == river_filter(s)
            {
                -1
            } else {
                RIVER
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerSmooth ---
struct Smooth {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for Smooth {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (mut v, n, e, wst, s) = g.cross(j, i);
            if wst == e && n == s {
                let mut r = self.seed.chunk(j + x, i + z);
                v = if r.next(2) == 0 { wst } else { n };
            } else {
                if wst == e {
                    v = wst;
                }
                if n == s {
                    v = n;
                }
            }
            v
        })
    }
    single_parent_seed!();
}

// --- GenLayerRareBiome ---
struct RareBiome {
    seed: Seed,
    parent: Layer,
}

impl GenLayer for RareBiome {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let mut r = self.seed.chunk(j + x, i + z);
            let v = g.at(j + 1, i + 1);
            if r.next(57) == 0 && v == PLAINS {
                PLAINS + 128
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerShore ---
struct Shore {
    seed: Seed,
    parent: Layer,
}

/// `GenLayerShore.func_151631_c`
fn jungle_compatible(id: i32) -> bool {
    class_of(id) == Some(Class::Jungle)
        || id == JUNGLE_EDGE
        || id == JUNGLE
        || id == JUNGLE_HILLS
        || id == FOREST
        || id == TAIGA
        || is_oceanic(id)
}

impl GenLayer for Shore {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        map3x3(&self.parent, x, z, w, h, |g, j, i| {
            let (v, n, e, wst, s) = g.cross(j, i);
            let nb = [n, e, wst, s];
            let any_ocean = nb.iter().any(|&t| is_oceanic(t));
            if v == MUSHROOM_ISLAND {
                if nb.iter().any(|&t| t == OCEAN) {
                    MUSHROOM_SHORE
                } else {
                    v
                }
            } else if class_of(v) == Some(Class::Jungle) {
                if nb.iter().all(|&t| jungle_compatible(t)) {
                    if any_ocean {
                        BEACH
                    } else {
                        v
                    }
                } else {
                    JUNGLE_EDGE
                }
            } else if v == EXTREME_HILLS || v == EXTREME_HILLS_PLUS || v == EXTREME_HILLS_EDGE {
                if !is_oceanic(v) && any_ocean {
                    STONE_BEACH
                } else {
                    v
                }
            } else if is_snowy(v) {
                if !is_oceanic(v) && any_ocean {
                    COLD_BEACH
                } else {
                    v
                }
            } else if v == MESA || v == MESA_PLATEAU_F {
                if !any_ocean {
                    if nb.iter().all(|&t| is_mesa(t)) {
                        v
                    } else {
                        DESERT
                    }
                } else {
                    v
                }
            } else if v != OCEAN && v != DEEP_OCEAN && v != RIVER && v != SWAMP {
                if any_ocean {
                    BEACH
                } else {
                    v
                }
            } else {
                v
            }
        })
    }
    single_parent_seed!();
}

// --- GenLayerRiverMix ---
struct RiverMix {
    seed: Seed,
    biome: Layer,
    river: Layer,
}

impl GenLayer for RiverMix {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let b = self.biome.get(x, z, w, h);
        let r = self.river.get(x, z, w, h);
        b.iter()
            .zip(r.iter())
            .map(|(&b, &r)| {
                if b != OCEAN && b != DEEP_OCEAN {
                    if r == RIVER {
                        if b == ICE_PLAINS {
                            FROZEN_RIVER
                        } else if b != MUSHROOM_ISLAND && b != MUSHROOM_SHORE {
                            r & 0xFF
                        } else {
                            MUSHROOM_SHORE
                        }
                    } else {
                        b
                    }
                } else {
                    b
                }
            })
            .collect()
    }
    fn init_world_seed(&mut self, world: i64) {
        self.biome.init_world_seed(world);
        self.river.init_world_seed(world);
        self.seed.init(world);
    }
}

// --- GenLayerVoronoiZoom ---
struct VoronoiZoom {
    seed: Seed,
    parent: Arc<dyn GenLayer>,
}

impl VoronoiZoom {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        let x = x - 2;
        let z = z - 2;
        let px = x >> 2;
        let pz = z >> 2;
        let pw = (w >> 2) + 2;
        let ph = (h >> 2) + 2;
        let p = self.parent.get(px, pz, pw, ph);
        let tw = ((pw - 1) << 2) as usize;
        let th = ((ph - 1) << 2) as usize;
        let mut tmp = vec![0; tw * th];
        let jit = |r: &mut Rng| (r.next(1024) as f64 / 1024.0 - 0.5) * 3.6;
        for k in 0..ph - 1 {
            let mut a = p[(k * pw) as usize];
            let mut c = p[((k + 1) * pw) as usize];
            for l in 0..pw - 1 {
                let mut r = self.seed.chunk((l + px) << 2, (k + pz) << 2);
                let d1 = jit(&mut r);
                let d2 = jit(&mut r);
                let mut r = self.seed.chunk((l + px + 1) << 2, (k + pz) << 2);
                let d3 = jit(&mut r) + 4.0;
                let d4 = jit(&mut r);
                let mut r = self.seed.chunk((l + px) << 2, (k + pz + 1) << 2);
                let d5 = jit(&mut r);
                let d6 = jit(&mut r) + 4.0;
                let mut r = self.seed.chunk((l + px + 1) << 2, (k + pz + 1) << 2);
                let d7 = jit(&mut r) + 4.0;
                let d8 = jit(&mut r) + 4.0;
                let b = p[(l + 1 + k * pw) as usize] & 0xFF;
                let d = p[(l + 1 + (k + 1) * pw) as usize] & 0xFF;
                for i4 in 0..4 {
                    let mut idx = (((k << 2) + i4) as usize) * tw + (l << 2) as usize;
                    let fi = i4 as f64;
                    for k4 in 0..4 {
                        let fk = k4 as f64;
                        let e9 = (fi - d2) * (fi - d2) + (fk - d1) * (fk - d1);
                        let e10 = (fi - d4) * (fi - d4) + (fk - d3) * (fk - d3);
                        let e11 = (fi - d6) * (fi - d6) + (fk - d5) * (fk - d5);
                        let e12 = (fi - d8) * (fi - d8) + (fk - d7) * (fk - d7);
                        tmp[idx] = if e9 < e10 && e9 < e11 && e9 < e12 {
                            a
                        } else if e10 < e9 && e10 < e11 && e10 < e12 {
                            b
                        } else if e11 < e9 && e11 < e10 && e11 < e12 {
                            c
                        } else {
                            d
                        };
                        idx += 1;
                    }
                }
                a = b;
                c = d;
            }
        }
        let mut out = vec![0; (w * h) as usize];
        for k in 0..h {
            let src = ((k + (z & 3)) as usize) * tw + (x & 3) as usize;
            out[(k * w) as usize..((k + 1) * w) as usize].copy_from_slice(&tmp[src..src + w as usize]);
        }
        out
    }
}

// ---------- 链 ----------

macro_rules! layer {
    ($t:ident, $seed:expr, $parent:expr) => {
        Box::new($t {
            seed: Seed::new($seed),
            parent: $parent,
        }) as Layer
    };
}

/// `Island … DeepOcean`。Java 里这棵树被 river/biome 两条支路共享同一批对象；
/// 层的世界种子只依赖 (世界种子, 层种子)，所以每条支路各建一份结果完全相同。
fn base_tree() -> Layer {
    let mut l: Layer = Box::new(Island {
        seed: Seed::new(1),
    });
    l = Box::new(Zoom {
        seed: Seed::new(2000),
        parent: l,
        fuzzy: true,
    });
    l = layer!(AddIsland, 1, l);
    l = zoom(2001, l);
    l = layer!(AddIsland, 2, l);
    l = layer!(AddIsland, 50, l);
    l = layer!(AddIsland, 70, l);
    l = layer!(RemoveTooMuchOcean, 2, l);
    l = layer!(AddSnow, 2, l);
    l = layer!(AddIsland, 3, l);
    for (seed, mode) in [
        (2, EdgeMode::CoolWarm),
        (2, EdgeMode::HeatIce),
        (3, EdgeMode::Special),
    ] {
        l = Box::new(Edge {
            seed: Seed::new(seed),
            parent: l,
            mode,
        });
    }
    l = zoom(2002, l);
    l = zoom(2003, l);
    l = layer!(AddIsland, 4, l);
    l = layer!(AddMushroomIsland, 5, l);
    l = layer!(DeepOcean, 4, l);
    magnify(1000, l, 0)
}

const BIOME_SIZE: i32 = 4;
const RIVER_SIZE: i32 = 4;

/// 构造并播种 `RiverMix`（1:4 分辨率的主层）
fn build_river_mix(world: i64) -> Layer {
    // Hills 的 river 输入：RiverInit 与主 river 支路共享（已播种），其上 2 层 Zoom 未播种
    let mut hills_river_init = layer!(RiverInit, 100, base_tree());
    hills_river_init.init_world_seed(world);
    let hills_river = magnify(1000, hills_river_init, 2);

    let mut main = layer!(BiomeInit, 200, base_tree());
    main = magnify(1000, main, 2);
    main = layer!(BiomeEdge, 1000, main);
    main = Box::new(Hills {
        seed: Seed::new(1000),
        parent: main,
        river: hills_river,
    });

    let mut river = layer!(RiverInit, 100, base_tree());
    river = magnify(1000, river, 2);
    river = magnify(1000, river, RIVER_SIZE);
    river = layer!(River, 1, river);
    river = layer!(Smooth, 1000, river);

    main = layer!(RareBiome, 1001, main);
    for i in 0..BIOME_SIZE {
        main = zoom(1000 + i as i64, main);
        if i == 0 {
            main = layer!(AddIsland, 3, main);
        }
        if i == 1 {
            main = layer!(Shore, 1000, main);
        }
    }
    main = layer!(Smooth, 1000, main);

    let mut mix: Layer = Box::new(RiverMix {
        seed: Seed::new(100),
        biome: main,
        river,
    });
    mix.init_world_seed(world);
    mix
}

/// biome 生成器：`main`（1:4）+ `voronoi`（1:1）。
#[derive(Clone)]
pub struct BiomeGen {
    main: Arc<dyn GenLayer>,
    voronoi: Arc<VoronoiZoom>,
    cache: Arc<Mutex<HashMap<(i32, i32), Arc<[u8; 256]>>>>,
}

impl BiomeGen {
    /// `seed` 是 MCPE 的 32 位世界种子（`BiomeSource(unsigned)` 零扩展成 `long long`）
    pub fn new(seed: i64) -> Self {
        let main: Arc<dyn GenLayer> = Arc::from(build_river_mix(seed));
        let mut vseed = Seed::new(10);
        vseed.init(seed);
        let voronoi = Arc::new(VoronoiZoom {
            seed: vseed,
            parent: main.clone(),
        });
        Self {
            main,
            voronoi,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 1:4 分辨率原始 biome（`getInts`），`[dx + dz * w]`
    pub fn raw(&self, x4: i32, z4: i32, w: i32, h: i32) -> Vec<i32> {
        self.main.get(x4, z4, w, h)
    }

    /// 1:1 分辨率（VoronoiZoom）`getInts`，`[dx + dz * w]`
    pub fn blocks(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        // Java 的 VoronoiZoom 只在 w/h 与对齐配合时才不越界（MCPE 只取 16×16 区块）；
        // 这里多取一圈再裁剪，结果只依赖绝对坐标，与窗口大小无关
        let (pw, ph) = ((w + 7) & !3, (h + 7) & !3);
        let v = self.voronoi.get(x, z, pw, ph);
        let mut out = Vec::with_capacity((w * h) as usize);
        for k in 0..h {
            out.extend_from_slice(&v[(k * pw) as usize..(k * pw + w) as usize]);
        }
        out
    }

    /// `fillRawBiomeData`：地形密度用的 10×10，`out[i][j]` = 网格 (x=i, z=j)，
    /// 覆盖 1:4 坐标 `(cx*4-2 .. cx*4+7, cz*4-2 .. cz*4+7)`
    pub fn raw_chunk(&self, cx: i32, cz: i32) -> [[u8; 10]; 10] {
        let v = self.raw(cx * 4 - 2, cz * 4 - 2, 10, 10);
        let mut out = [[0u8; 10]; 10];
        for i in 0..10 {
            for j in 0..10 {
                out[i][j] = (v[i + j * 10] & 0xFF) as u8;
            }
        }
        out
    }

    /// 每方块 biome（`fillBiomeData`），索引 `x + z * 16`，带区块缓存
    pub fn chunk_biomes(&self, cx: i32, cz: i32) -> Arc<[u8; 256]> {
        if let Some(v) = self.cache.lock().unwrap().get(&(cx, cz)) {
            return v.clone();
        }
        let src = self.blocks(cx * 16, cz * 16, 16, 16);
        let mut out = [0u8; 256];
        for (o, s) in out.iter_mut().zip(src.iter()) {
            *o = (*s & 0xFF) as u8;
        }
        let v: Arc<[u8; 256]> = Arc::new(out);
        let mut cache = self.cache.lock().unwrap();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert((cx, cz), v.clone());
        v
    }

    /// 单个方块的 biome
    pub fn biome_at(&self, x: i32, z: i32) -> u8 {
        self.chunk_biomes(x >> 4, z >> 4)[(x & 15) as usize + (z & 15) as usize * 16]
    }
}

impl GenLayer for VoronoiZoom {
    fn get(&self, x: i32, z: i32, w: i32, h: i32) -> Vec<i32> {
        VoronoiZoom::get(self, x, z, w, h)
    }
    fn init_world_seed(&mut self, world: i64) {
        self.seed.init(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcg_matches_java() {
        // new GenLayer(1).baseSeed，Java 下用 long 溢出计算
        let mut s: i64 = 1;
        for _ in 0..3 {
            s = s
                .wrapping_mul(s.wrapping_mul(A).wrapping_add(B))
                .wrapping_add(1);
        }
        assert_eq!(base_seed(1), s);
    }

    #[test]
    fn deterministic_and_has_ocean_and_land() {
        let g = BiomeGen::new(57536720);
        let a = g.raw(-64, -64, 128, 128);
        let b = BiomeGen::new(57536720).raw(-64, -64, 128, 128);
        assert_eq!(a, b);
        let ocean = a.iter().filter(|&&v| is_oceanic(v)).count();
        assert!(ocean > 0 && ocean < a.len(), "ocean={ocean}");
        assert!(a.iter().all(|&v| exists(v)), "未注册的 biome id");
    }

    #[test]
    fn voronoi_consistent_with_chunks() {
        let g = BiomeGen::new(1152860609);
        let c = g.chunk_biomes(3, -2);
        let direct = g.voronoi.get(48, -32, 16, 16);
        for k in 0..256 {
            assert_eq!(c[k] as i32, direct[k]);
        }
    }

    #[test]
    fn zoom_window_independent() {
        // 同一位置用不同窗口取值必须一致（坐标/对齐正确的必要条件）
        let g = BiomeGen::new(12345);
        let big = g.raw(-40, -40, 80, 80);
        let small = g.raw(3, 7, 5, 5);
        for i in 0..5 {
            for j in 0..5 {
                assert_eq!(small[(j + i * 5) as usize], big[(43 + j + (47 + i) * 80) as usize]);
            }
        }
    }
}
