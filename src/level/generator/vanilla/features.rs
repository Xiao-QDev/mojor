//! 装饰阶段用到的全部 Feature（MCPE 0.14.1 `levelgen/feature/*.cpp`）。
//!
//! [`Src`] 对应 postProcess 里那个 `BlockSource(level, dim, view, false, true)`：
//! 只能访问以中心区块为中心的 3×3 区块；写方块不触发任何事件（没有 onPlace、
//! 没有邻居更新、高度图不变），高度图读的是区块里存的那份。

use super::blockprops::{self as bp, mat, VINE_EAST, VINE_NORTH, VINE_SOUTH, VINE_WEST};
use super::liquid::TickQueue;
use crate::level::generator::mcpe_noise::McpeRandom;
use crate::level::generator::world::GenWorld;

pub const PI: f32 = 3.14159274;

static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// 构造参数里几个 `nextInt` 的求值顺序：ARM 版（真实存档，0.14.1，重建源码的语句顺序）从左到右；
/// x86 版 0.14.3（模拟器对拍用）从右到左。默认 ARM。
pub static X86_ORDER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[inline]
pub fn x86_order() -> bool {
    X86_ORDER.load(std::sync::atomic::Ordering::Relaxed)
}

static ARM_SWAP: std::sync::OnceLock<u64> = std::sync::OnceLock::new();

/// ARM 版里哪些调用点同样是先 z 后 x（位掩码，按 [`xyz`] 的 site 编号）
pub fn arm_swap_mask() -> u64 {
    *ARM_SWAP.get_or_init(|| {
        std::env::var("ARM_SWAP").ok().and_then(|v| u64::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(ARM_SWAP_DEFAULT)
    })
}

/// ARM 版（真实存档）里实测为"先 z"的调用点
pub const ARM_SWAP_DEFAULT: u64 = 0;

/// 按目标平台的求值顺序取三个分量（源码顺序 x、y、z）。`site` 为调用点编号。
#[inline]
pub fn xyz<A, B, C>(
    site: u32,
    r: &mut McpeRandom,
    fx: impl FnOnce(&mut McpeRandom) -> A,
    fy: impl FnOnce(&mut McpeRandom) -> B,
    fz: impl FnOnce(&mut McpeRandom) -> C,
) -> (A, B, C) {
    if x86_order() || arm_swap_mask() >> site & 1 != 0 {
        let z = fz(r);
        let y = fy(r);
        let x = fx(r);
        (x, y, z)
    } else {
        let x = fx(r);
        let y = fy(r);
        let z = fz(r);
        (x, y, z)
    }
}

/// 对拍用：环境变量 TRACE=1 时打印 Feature::place 调用（与 decocheck.py 的 TRACE 同格式）
#[inline]
pub fn trace(name: &str, p: (i32, i32, i32), r: &McpeRandom) {
    if *TRACE.get_or_init(|| std::env::var("TRACE").is_ok()) {
        let mut t = r.clone();
        println!("TRACE {name} ({}, {}, {}) {} {}", p.0, p.1, p.2, r.mti(), t.genrand());
    }
}

/// `Math::sin` / `Math::cos`：65536 项查表
pub struct SinTable(Box<[f32; 65536]>);

impl SinTable {
    fn new() -> Self {
        let mut t = Box::new([0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = ((i as f32 * 9.58738019e-05f32) as f64).sin() as f32;
        }
        SinTable(t)
    }
}

static SIN: std::sync::OnceLock<SinTable> = std::sync::OnceLock::new();

#[inline]
pub fn msin(v: f32) -> f32 {
    let t = SIN.get_or_init(SinTable::new);
    t.0[((v * 10430.3779f32) as i32 & 0xffff) as usize]
}
#[inline]
pub fn mcos(v: f32) -> f32 {
    let t = SIN.get_or_init(SinTable::new);
    t.0[((v * 10430.3779f32 + 16384.0f32) as i32 & 0xffff) as usize]
}

// 方块 id
pub const AIR: u8 = 0;
pub const STONE: u8 = 1;
pub const GRASS: u8 = 2;
pub const DIRT: u8 = 3;
pub const COBBLE: u8 = 4;
pub const FLOWING_WATER: u8 = 8;
pub const STILL_WATER: u8 = 9;
pub const FLOWING_LAVA: u8 = 10;
pub const STILL_LAVA: u8 = 11;
pub const SAND: u8 = 12;
pub const GRAVEL: u8 = 13;
pub const LOG: u8 = 17;
pub const LEAVES: u8 = 18;
pub const SANDSTONE: u8 = 24;
pub const TALLGRASS: u8 = 31;
pub const DEADBUSH: u8 = 32;
pub const YELLOW_FLOWER: u8 = 37;
pub const RED_FLOWER: u8 = 38;
pub const BROWN_MUSHROOM: u8 = 39;
pub const RED_MUSHROOM: u8 = 40;
pub const STONE_SLAB: u8 = 44;
pub const MOSSY_COBBLE: u8 = 48;
pub const OBSIDIAN: u8 = 49;
pub const FIRE: u8 = 51;
pub const FARMLAND: u8 = 60;
pub const SNOW: u8 = 80;
pub const ICE: u8 = 79;
pub const CACTUS: u8 = 81;
pub const CLAY: u8 = 82;
pub const REEDS: u8 = 83;
pub const PUMPKIN: u8 = 86;
pub const INVISIBLE_BEDROCK: u8 = 95;
pub const MONSTER_EGG: u8 = 97;
pub const BROWN_MUSHROOM_BLOCK: u8 = 99;
pub const MELON: u8 = 103;
pub const VINE: u8 = 106;
pub const MYCELIUM: u8 = 110;
pub const WATERLILY: u8 = 111;
pub const EMERALD_ORE: u8 = 129;
pub const COCOA: u8 = 127;
pub const STAINED_CLAY: u8 = 159;
pub const LEAVES2: u8 = 161;
pub const LOG2: u8 = 162;
pub const HARDENED_CLAY: u8 = 172;
pub const PACKED_ICE: u8 = 174;
pub const DOUBLE_PLANT: u8 = 175;
pub const PODZOL: u8 = 243;

/// postProcess 的 BlockSource
pub struct Src<'w, 'a> {
    pub world: &'w mut GenWorld<'a>,
    pub cx: i32,
    pub cz: i32,
    pub queue: TickQueue,
}

impl<'w, 'a> Src<'w, 'a> {
    pub fn new(world: &'w mut GenWorld<'a>, cx: i32, cz: i32) -> Self {
        Self {
            world,
            cx,
            cz,
            queue: TickQueue::new(),
        }
    }

    /// 区块在 3×3 视图里（且已生成）
    #[inline]
    pub fn has_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        (chunk_x - self.cx).abs() <= 1 && (chunk_z - self.cz).abs() <= 1 && self.world.contains(chunk_x, chunk_z)
    }
    #[inline]
    pub fn has_block(&self, x: i32, z: i32) -> bool {
        self.has_chunk(x >> 4, z >> 4)
    }
    /// `BlockSource::hasChunksAt(min, max)`
    pub fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        for cx in (x0 >> 4)..=(x1 >> 4) {
            for cz in (z0 >> 4)..=(z1 >> 4) {
                if !self.has_chunk(cx, cz) {
                    return false;
                }
            }
        }
        true
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..128).contains(&y) || !self.has_block(x, z) {
            return AIR;
        }
        self.world.block(x, y, z)
    }
    #[inline]
    pub fn data(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..128).contains(&y) || !self.has_block(x, z) {
            return 0;
        }
        self.world.meta(x, y, z)
    }
    #[inline]
    pub fn mat(&self, x: i32, y: i32, z: i32) -> u8 {
        bp::material(self.get(x, y, z))
    }
    #[inline]
    pub fn is_empty(&self, x: i32, y: i32, z: i32) -> bool {
        self.get(x, y, z) == AIR
    }

    /// `BlockSource::setBlockAndData`（不触发事件）。返回值同原版。
    pub fn set(&mut self, x: i32, y: i32, z: i32, id: u8, data: u8) -> bool {
        if !(0..128).contains(&y) || !self.has_block(x, z) {
            return false;
        }
        let old_id = self.world.block(x, y, z);
        let old_data = self.world.meta(x, y, z);
        self.world.set_block(x, y, z, id);
        self.world.set_meta(x, y, z, data & 0xf);
        if old_id == id {
            return old_data != (data & 0xf);
        }
        true
    }

    /// `BlockSource::getHeightmap`：区块里存的高度图（postProcess 期间不更新）
    pub fn heightmap(&self, x: i32, z: i32) -> i32 {
        if !self.has_block(x, z) {
            return 0;
        }
        self.world
            .chunk(x >> 4, z >> 4)
            .map(|c| c.height((x & 15) as usize, (z & 15) as usize) as i32)
            .unwrap_or(0)
    }

    /// `BlockSource::getTopSolidBlock(x, z, false)`
    pub fn top_solid(&self, x: i32, z: i32) -> i32 {
        if !self.has_block(x, z) {
            return 255;
        }
        let mut height: i32 = 127;
        loop {
            let id = self.get(x, height, z);
            height -= 1;
            let skip = bp::material(id) == mat::LEAVES || !bp::blocks_motion(id);
            if !(skip && height != 0) {
                break;
            }
        }
        if height != 0 {
            height + 2
        } else {
            0
        }
    }

    pub fn biome(&self, x: i32, z: i32) -> u8 {
        self.world.biome(x, z)
    }

    pub fn solid_blocking_block(&self, x: i32, y: i32, z: i32) -> bool {
        bp::solid_blocking_block(self.get(x, y, z))
    }

    /// 列天空光（recalcHeightmap 的算法，只看本列）
    pub fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        let mut light = 15;
        let mut yy = 127;
        while yy >= y {
            light -= bp::light_block(self.get(x, yy, z));
            if light <= 0 {
                return 0;
            }
            yy -= 1;
        }
        light
    }
}

// ---------------------------------------------------------------- 方块存活规则

/// `BushBlock::mayPlaceOn`
fn bush_soil(id: u8) -> bool {
    matches!(id, GRASS | DIRT | FARMLAND | PODZOL)
}

/// `Block::mayPlace(source, pos)` 里不依赖 mayPlaceOn 的部分
fn base_may_place(s: &Src, x: i32, y: i32, z: i32) -> bool {
    (0..=127).contains(&y) && bp::replaceable(s.get(x, y, z))
}

/// `Block::mBlocks[id]->canSurvive(source, pos)`（装饰会放的植物）
pub fn can_survive(s: &Src, id: u8, x: i32, y: i32, z: i32) -> bool {
    match id {
        YELLOW_FLOWER | RED_FLOWER | TALLGRASS | 6 => bush_soil(s.get(x, y - 1, z)),
        DEADBUSH => matches!(s.get(x, y - 1, z), SAND | HARDENED_CLAY | STAINED_CLAY | PODZOL),
        BROWN_MUSHROOM | RED_MUSHROOM => {
            if !(0..128).contains(&y) {
                return false;
            }
            let below = s.get(x, y - 1, z);
            below == PODZOL || below == MYCELIUM || (s.sky_light(x, y, z) < 13 && bp::msolid(below))
        }
        _ => true,
    }
}

// ---------------------------------------------------------------- 简单 Feature

/// `OreFeature::place`
pub fn ore(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, block: u8, data: u8, size: i32) {
    trace("OreFeature", pos, r);
    let angle = r.next_float() * PI;
    let sx = msin(angle) * size as f32 / 8.0;
    let sz = mcos(angle) * size as f32 / 8.0;
    let (px, py, pz) = (pos.0 as f32, pos.1 as f32, pos.2 as f32);
    let fy = r.next_int(3) as f32 - 2.0;
    let from = (px + (sx + 8.0), py + fy, pz + (sz + 8.0));
    let ty = r.next_int(3) as f32 - 2.0;
    let to = (px + (8.0 - sx), py + ty, pz + (8.0 - sz));
    for i in 0..size {
        let t = i as f32 / size as f32;
        let c = (
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
            from.2 + (to.2 - from.2) * t,
        );
        let rr = r.next_float() * size as f32 / 16.0;
        let d = (msin(i as f32 * PI / size as f32) + 1.0) * rr + 1.0;
        let h = d * 0.5;
        let inv = 2.0 / d;
        let lo = ((c.0 - h).floor() as i32, (c.1 - h).floor() as i32, (c.2 - h).floor() as i32);
        let hi = ((h + c.0).floor() as i32, (h + c.1).floor() as i32, (h + c.2).floor() as i32);
        for x in lo.0..=hi.0 {
            let dx = (x as f32 + 0.5 - c.0) * inv;
            if dx * dx >= 1.0 {
                continue;
            }
            for y in lo.1..=hi.1 {
                let dy = (y as f32 + 0.5 - c.1) * inv;
                if dx * dx + dy * dy >= 1.0 {
                    continue;
                }
                for z in lo.2..=hi.2 {
                    let dz = (z as f32 + 0.5 - c.2) * inv;
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        let id = s.get(x, y, z);
                        if id == STONE || id == 87 {
                            s.set(x, y, z, block, data);
                        }
                    }
                }
            }
        }
    }
}

/// `SandFeature::place`（沙子、砂砾）
pub fn sand_disk(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, block: u8, radius: u32) -> bool {
    trace("SandFeature", pos, r);
    if s.mat(pos.0, pos.1, pos.2) != mat::WATER {
        return false;
    }
    let rad = (r.next_int(radius - 2) + 2) as i32;
    for x in pos.0 - rad..=pos.0 + rad {
        for z in pos.2 - rad..=pos.2 + rad {
            let (xd, zd) = (x - pos.0, z - pos.2);
            if xd * xd + zd * zd <= rad * rad {
                for y in pos.1 - 2..=pos.1 + 2 {
                    let id = s.get(x, y, z);
                    if id == DIRT || id == GRASS {
                        s.set(x, y, z, block, 0);
                    }
                }
            }
        }
    }
    true
}

/// `ClayFeature::place`
pub fn clay(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, count: u32) -> bool {
    trace("ClayFeature", pos, r);
    let id = s.get(pos.0, pos.1, pos.2);
    if bp::material(id) != mat::WATER && id != CLAY {
        return false;
    }
    let rad = (r.next_int(count - 2) + 2) as i32;
    for x in pos.0 - rad..=pos.0 + rad {
        for z in pos.2 - rad..=pos.2 + rad {
            let (xd, zd) = (x - pos.0, z - pos.2);
            if xd * xd + zd * zd <= rad * rad {
                for y in pos.1 - 1..=pos.1 + 1 {
                    if s.get(x, y, z) == DIRT {
                        s.set(x, y, z, CLAY, 0);
                    }
                }
            }
        }
    }
    true
}

/// 同 spread8，但 x86 版是先 z、再 y、后 x（见 [`xyz`]）
#[inline]
fn spread8_zyx(site: u32, r: &mut McpeRandom) -> (i32, i32, i32) {
    xyz(
        site,
        r,
        |r| r.next_int(8) as i32 - r.next_int(8) as i32,
        |r| r.next_int(4) as i32 - r.next_int(4) as i32,
        |r| r.next_int(8) as i32 - r.next_int(8) as i32,
    )
}

#[inline]
fn spread8(r: &mut McpeRandom) -> (i32, i32, i32) {
    let dx = r.next_int(8) as i32 - r.next_int(8) as i32;
    let dy = r.next_int(4) as i32 - r.next_int(4) as i32;
    let dz = r.next_int(8) as i32 - r.next_int(8) as i32;
    (dx, dy, dz)
}

/// `FlowerFeature::placeFlower`
pub fn place_flower(s: &mut Src, pos: (i32, i32, i32), id: u8, data: u8, r: &mut McpeRandom) {
    trace("placeFlower", pos, r);
    for _ in 0..64 {
        let (dx, dy, dz) = spread8_zyx(11, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if !s.is_empty(x, y, z) {
            continue;
        }
        if !can_survive(s, id, x, y, z) {
            continue;
        }
        s.set(x, y, z, id, data);
    }
}

/// `TallGrassFeature::place(source, pos, random, count, range)`
pub fn tall_grass(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, data: u8, count: i32, range: u32) {
    if count == 90 {
        trace("TallGrassFeature", pos, r);
    }
    let (px, mut py, pz) = pos;
    loop {
        let id = s.get(px, py, pz);
        if !((id == AIR || bp::props(id) & bp::PROP_LEAVES != 0) && py > 0) {
            break;
        }
        py -= 1;
    }
    for _ in 0..count {
        let (dx, dy, dz) = xyz(
            17,            r,
            |r| r.next_int(range) as i32 - r.next_int(range) as i32,
            |r| r.next_int(4) as i32 - r.next_int(4) as i32,
            |r| r.next_int(range) as i32 - r.next_int(range) as i32,
        );
        let (x, y, z) = (px + dx, py + dy, pz + dz);
        if s.get(x, y, z) == AIR && can_survive(s, TALLGRASS, x, y, z) {
            s.set(x, y, z, TALLGRASS, data);
        }
    }
}

/// `DoublePlantFeature::place(source, pos, random, data)`
pub fn double_plant(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, data: u8) -> bool {
    let mut placed = false;
    for _ in 0..64 {
        let (dx, dy, dz) = spread8_zyx(10, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if s.is_empty(x, y, z)
            && y <= 125
            && base_may_place(s, x, y, z)
            && bush_soil(s.get(x, y - 1, z))
            && s.is_empty(x, y + 1, z)
        {
            placed = true;
            s.set(x, y, z, DOUBLE_PLANT, data);
            s.set(x, y + 1, z, DOUBLE_PLANT, data | 8);
        }
    }
    placed
}

/// `DeadBushFeature::place`
pub fn dead_bush(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("DeadBushFeature", pos, r);
    let (px, mut py, pz) = pos;
    let mut m: Option<u8> = None;
    while m.map_or(true, |m| m == mat::AIR || m == mat::LEAVES) && py > 0 {
        m = Some(s.mat(px, py, pz));
        py -= 1;
    }
    for _ in 0..4 {
        let (dx, dy, dz) = spread8_zyx(13, r);
        let (x, y, z) = (px + dx, py + dy, pz + dz);
        if s.is_empty(x, y, z) && can_survive(s, DEADBUSH, x, y, z) {
            s.set(x, y, z, DEADBUSH, 0);
        }
    }
}

/// `ReedBlock::mayPlace` / canSurvive
fn reed_ok(s: &Src, x: i32, y: i32, z: i32) -> bool {
    let below = s.get(x, y - 1, z);
    if below == REEDS {
        return true;
    }
    if below == GRASS || below == DIRT || below == SAND {
        return s.mat(x - 1, y - 1, z) == mat::WATER
            || s.mat(x + 1, y - 1, z) == mat::WATER
            || s.mat(x, y - 1, z - 1) == mat::WATER
            || s.mat(x, y - 1, z + 1) == mat::WATER;
    }
    false
}

/// `ReedsFeature::place`
pub fn reeds(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("ReedsFeature", pos, r);
    for _ in 0..20 {
        let (dx, _, dz) = xyz(
            18,            r,
            |r| r.next_int(4) as i32 - r.next_int(4) as i32,
            |_| (),
            |r| r.next_int(4) as i32 - r.next_int(4) as i32,
        );
        let (x, y, z) = (pos.0 + dx, pos.1, pos.2 + dz);
        if !s.is_empty(x, y, z) {
            continue;
        }
        let by = y - 1;
        if s.mat(x - 1, by, z) != mat::WATER
            && s.mat(x + 1, by, z) != mat::WATER
            && s.mat(x, by, z - 1) != mat::WATER
            && s.mat(x, by, z + 1) != mat::WATER
        {
            continue;
        }
        let inner = r.next_int(3) + 1;
        let height = r.next_int(inner) as i32 + 2;
        for level in 0..height {
            if !reed_ok(s, x, y + level, z) {
                continue;
            }
            s.set(x, y + level, z, REEDS, 0);
        }
    }
}

/// `CactusBlock::canSurvive`
fn cactus_ok(s: &Src, x: i32, y: i32, z: i32) -> bool {
    if bp::mat_solid(s.get(x - 1, y, z))
        || bp::mat_solid(s.get(x + 1, y, z))
        || bp::mat_solid(s.get(x, y, z - 1))
        || bp::mat_solid(s.get(x, y, z + 1))
    {
        return false;
    }
    let below = s.get(x, y - 1, z);
    below == CACTUS || below == SAND
}

/// `CactusFeature::place`
pub fn cactus(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("CactusFeature", pos, r);
    for _ in 0..10 {
        let (dx, dy, dz) = spread8_zyx(14, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if s.is_empty(x, y, z) {
            let inner = r.next_int(3) + 1;
            let height = 1 + r.next_int(inner) as i32;
            for h in 0..height {
                if cactus_ok(s, x, y + h, z) {
                    s.set(x, y + h, z, CACTUS, 0);
                }
            }
        }
    }
}

/// `PumpkinFeature::place`
pub fn pumpkin(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("PumpkinFeature", pos, r);
    for _ in 0..64 {
        let (dx, dy, dz) = spread8_zyx(15, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if s.is_empty(x, y, z) && s.get(x, y - 1, z) == GRASS && {
            let id = s.get(x, y, z);
            (id == AIR || bp::replaceable(id)) && bp::top_solid_blocking(s.get(x, y - 1, z), s.data(x, y - 1, z))
        } {
            let d = r.next_int(4) as u8;
            s.set(x, y, z, PUMPKIN, d);
        }
    }
}

/// `MelonFeature::place`
pub fn melon(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("MelonFeature", pos, r);
    for _ in 0..64 {
        let (dx, dy, dz) = spread8_zyx(16, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if base_may_place(s, x, y, z) && s.get(x, y - 1, z) == GRASS {
            s.set(x, y, z, MELON, 0);
        }
    }
}

/// `WaterlilyFeature::place`
pub fn waterlily(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("WaterlilyFeature", pos, r);
    for _ in 0..10 {
        let (dx, dy, dz) = spread8_zyx(12, r);
        let (x, y, z) = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
        if s.is_empty(x, y, z) && base_may_place(s, x, y, z) && s.get(x, y - 1, z) == STILL_WATER {
            s.set(x, y, z, WATERLILY, 0);
        }
    }
}

/// `SpringFeature::place`
pub fn spring(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, block: u8) {
    trace("SpringFeature", pos, r);
    let (x, y, z) = pos;
    if s.get(x, y + 1, z) != STONE || s.get(x, y - 1, z) != STONE {
        return;
    }
    let here = s.get(x, y, z);
    if here != AIR && here != STONE {
        return;
    }
    let mut stone = 0;
    let mut empty = 0;
    for (ax, az) in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
        if s.get(ax, y, az) == STONE {
            stone += 1;
        }
    }
    for (ax, az) in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
        if s.is_empty(ax, y, az) {
            empty += 1;
        }
    }
    if empty == 1 && stone == 3 {
        if !s.set(x, y, z, block, 0) {
            return;
        }
        super::liquid::tick(s, (x, y, z), block, r);
    }
}

/// `BlockBlobFeature::place`
pub fn block_blob(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, block: u8, radius: i32) -> bool {
    trace("BlockBlobFeature", pos, r);
    let (mut x, mut y, mut z) = pos;
    while y > 3 {
        if !s.is_empty(x, y - 1, z) {
            let id = s.get(x, y - 1, z);
            if id == GRASS || id == DIRT || id == STONE {
                break;
            }
        }
        y -= 1;
    }
    if y <= 3 {
        return false;
    }
    let mut i = 0;
    while radius >= 0 && i < 3 {
        let rx = radius + r.next_int(2) as i32;
        let ry = radius + r.next_int(2) as i32;
        let rz = radius + r.next_int(2) as i32;
        let rad = 0.5f32 + (rx + ry + rz) as f32 * 0.333f32;
        for bx in x - rx..x + rx + 1 {
            for by in y - ry..y + ry + 1 {
                for bz in z - rz..z + rz + 1 {
                    let dx = (bx - x) as f32;
                    let dy = (by - y) as f32;
                    let dz = (bz - z) as f32;
                    if dx * dx + dy * dy + dz * dz <= rad * rad {
                        s.set(bx, by, bz, block, 0);
                    }
                }
            }
        }
        let (ox, oy, oz) = xyz(
            19,            r,
            |r| -(radius + 1) + r.next_int((2 + radius * 2) as u32) as i32,
            |r| r.next_int(2) as i32,
            |r| -(radius + 1) + r.next_int((2 + radius * 2) as u32) as i32,
        );
        x += ox;
        y -= oy;
        z += oz;
        i += 1;
    }
    true
}

/// `IcePatchFeature::place`
pub fn ice_patch(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, radius: u32) -> bool {
    trace("IcePatchFeature", pos, r);
    let (px, mut py, pz) = pos;
    while s.is_empty(px, py, pz) && py > 2 {
        py -= 1;
    }
    if s.get(px, py, pz) != SNOW {
        return false;
    }
    let rad = (r.next_int(radius - 2) + 2) as i32;
    for x in px - rad..px + rad + 1 {
        for z in pz - rad..pz + rad + 1 {
            let (xd, zd) = (x - px, z - pz);
            if xd * xd + zd * zd <= rad * rad {
                for y in py - 1..py + 2 {
                    let id = s.get(x, y, z);
                    if id == DIRT || id == SNOW || id == ICE {
                        s.set(x, y, z, PACKED_ICE, 0);
                    }
                }
            }
        }
    }
    true
}

fn ice_spike_ok(id: u8) -> bool {
    id == AIR || id == DIRT || id == SNOW || id == ICE
}

/// `IceSpikeFeature::place`
pub fn ice_spike(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    trace("IceSpikeFeature", pos, r);
    let (bx, mut by, bz) = pos;
    while s.is_empty(bx, by, bz) && by > 2 {
        by -= 1;
    }
    if s.get(bx, by, bz) != SNOW {
        return false;
    }
    by += r.next_int(4) as i32;
    let height = r.next_int(4) as i32 + 7;
    let radius = height / 4 + r.next_int(2) as i32;
    if radius > 1 && r.next_int(60) == 0 {
        by += 10 + r.next_int(30) as i32;
    }
    for level in 0..height {
        let scale = (1.0f32 - level as f32 / height as f32) * radius as f32;
        let rr = scale.ceil() as i32;
        for dx in -rr..rr + 1 {
            let fx = dx.abs() as f32 - 0.25f32;
            for dz in -rr..rr + 1 {
                let fz = dz.abs() as f32 - 0.25f32;
                if ((dx == 0 && dz == 0) || fx * fx + fz * fz <= scale * scale)
                    && ((dx != -rr && dx != rr && dz != -rr && dz != rr) || r.next_float() <= 0.75f32)
                {
                    let id = s.get(bx + dx, by + level, bz + dz);
                    if ice_spike_ok(id) {
                        s.set(bx + dx, by + level, bz + dz, PACKED_ICE, 0);
                    }
                    if level != 0 && rr > 1 {
                        let id = s.get(bx + dx, by - level, bz + dz);
                        if ice_spike_ok(id) {
                            s.set(bx + dx, by - level, bz + dz, PACKED_ICE, 0);
                        }
                    }
                }
            }
        }
    }
    let span = (radius - 1).clamp(0, 1);
    for dx in -span..span + 1 {
        for dz in -span..span + 1 {
            let (cx, mut cy, cz) = (bx + dx, by - 1, bz + dz);
            let mut left = 50;
            if dx.abs() == 1 && dz.abs() == 1 {
                left = r.next_int(5) as i32;
            }
            while cy > 50 {
                let id = s.get(cx, cy, cz);
                if !ice_spike_ok(id) && id != PACKED_ICE {
                    break;
                }
                s.set(cx, cy, cz, PACKED_ICE, 0);
                cy -= 1;
                left -= 1;
                if left <= 0 {
                    cy -= r.next_int(5) as i32 + 1;
                    left = r.next_int(5) as i32;
                }
            }
        }
    }
    true
}

/// `DesertWellFeature::place`
pub fn desert_well(s: &mut Src, pos: (i32, i32, i32)) -> bool {
    let (px, mut py, pz) = pos;
    while s.is_empty(px, py, pz) && py > 2 {
        py -= 1;
    }
    if s.get(px, py, pz) != SAND {
        return false;
    }
    for dx in -2..=2 {
        for dz in -2..=2 {
            if s.is_empty(px + dx, py - 1, pz + dz) && s.is_empty(px + dx, py - 2, pz + dz) {
                return false;
            }
        }
    }
    for dy in -1..=0 {
        for dx in -2..=2 {
            for dz in -2..=2 {
                s.set(px + dx, py + dy, pz + dz, SANDSTONE, 0);
            }
        }
    }
    s.set(px, py, pz, STILL_WATER, 0);
    s.set(px - 1, py, pz, STILL_WATER, 0);
    s.set(px + 1, py, pz, STILL_WATER, 0);
    s.set(px, py, pz - 1, STILL_WATER, 0);
    s.set(px, py, pz + 1, STILL_WATER, 0);
    for dx in -2..=2 {
        for dz in -2..=2 {
            if dx == -2 || dx == 2 || dz == -2 || dz == 2 {
                s.set(px + dx, py + 1, pz + dz, SANDSTONE, 0);
            }
        }
    }
    s.set(px + 2, py + 1, pz, STONE_SLAB, 1);
    s.set(px - 2, py + 1, pz, STONE_SLAB, 1);
    s.set(px, py + 1, pz + 2, STONE_SLAB, 1);
    s.set(px, py + 1, pz - 2, STONE_SLAB, 1);
    for dx in -1..=1 {
        for dz in -1..=1 {
            if dx == 0 && dz == 0 {
                s.set(px + dx, py + 4, pz + dz, SANDSTONE, 0);
            } else {
                s.set(px + dx, py + 4, pz + dz, STONE_SLAB, 1);
            }
        }
    }
    for dy in 1..=3 {
        s.set(px - 1, py + dy, pz - 1, SANDSTONE, 0);
        s.set(px - 1, py + dy, pz + 1, SANDSTONE, 0);
        s.set(px + 1, py + dy, pz - 1, SANDSTONE, 0);
        s.set(px + 1, py + dy, pz + 1, SANDSTONE, 0);
    }
    true
}

fn huge_mushroom_soil(id: u8) -> bool {
    id == DIRT || id == GRASS || id == MYCELIUM
}

/// `HugeMushroomFeature::place`（type = -1：随机）
pub fn huge_mushroom(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    trace("HugeMushroomFeature", pos, r);
    let (ox, mut oy, oz) = pos;
    let kind = r.next_int(2) as i32;
    let mut height = r.next_int(3) as i32 + 4;
    if r.next_int(12) == 0 {
        height *= 2;
    }
    if oy < 1 || oy + height + 1 >= 128 {
        return false;
    }
    let mut free = true;
    for y in oy..=oy + 1 + height {
        let radius = if y <= oy + 3 { 0 } else { 3 };
        let mut x = ox - radius;
        while x <= ox + radius && free {
            let mut z = oz - radius;
            while z <= oz + radius && free {
                if (0..128).contains(&y) {
                    let m = s.mat(x, y, z);
                    if m != mat::AIR && m != mat::LEAVES {
                        free = false;
                    }
                } else {
                    free = false;
                }
                z += 1;
            }
            x += 1;
        }
    }
    if !free {
        return false;
    }
    let below = s.get(ox, oy - 1, oz);
    if !huge_mushroom_soil(below) {
        // 沼泽（BiomeType 15）里可以长在浅水上
        if !is_swamp_type(s.biome(ox, oz)) {
            return false;
        }
        let two = s.get(ox, oy - 2, oz);
        let three = s.get(ox, oy - 3, oz);
        if below != STILL_WATER || (!huge_mushroom_soil(two) && (two != STILL_WATER || !huge_mushroom_soil(three))) {
            return false;
        }
        let depth = if two == STILL_WATER { 2 } else { 1 };
        height += depth;
        oy -= depth;
    }
    let mut cap_start = oy + height;
    if kind == 1 {
        cap_start = oy + height - 3;
    }
    let block = BROWN_MUSHROOM_BLOCK + kind as u8;
    for y in cap_start..=oy + height {
        let mut radius = 1;
        if y < oy + height {
            radius += 1;
        }
        if kind == 0 {
            radius = 3;
        }
        for x in ox - radius..=ox + radius {
            for z in oz - radius..=oz + radius {
                let mut data = 5;
                if x == ox - radius {
                    data -= 1;
                }
                if x == ox + radius {
                    data += 1;
                }
                if z == oz - radius {
                    data -= 3;
                }
                if z == oz + radius {
                    data += 3;
                }
                if kind == 0 || y < oy + height {
                    if (x == ox - radius || x == ox + radius) && (z == oz - radius || z == oz + radius) {
                        continue;
                    }
                    if x == ox - (radius - 1) && z == oz - radius {
                        data = 1;
                    }
                    if x == ox - radius && z == oz - (radius - 1) {
                        data = 1;
                    }
                    if x == ox + (radius - 1) && z == oz - radius {
                        data = 3;
                    }
                    if x == ox + radius && z == oz - (radius - 1) {
                        data = 3;
                    }
                    if x == ox - (radius - 1) && z == oz + radius {
                        data = 7;
                    }
                    if x == ox - radius && z == oz + (radius - 1) {
                        data = 7;
                    }
                    if x == ox + (radius - 1) && z == oz + radius {
                        data = 9;
                    }
                    if x == ox + radius && z == oz + (radius - 1) {
                        data = 9;
                    }
                }
                if data == 5 && y < oy + height {
                    data = 0;
                }
                // 原版比较的是 origin.y（不是 y）：data 0 的格子一律跳过
                if (data == 0 && oy < oy + height - 1) || s.solid_blocking_block(x, y, z) {
                    continue;
                }
                s.set(x, y, z, block, data as u8);
            }
        }
    }
    for i in 0..height {
        if !s.solid_blocking_block(ox, oy + i, oz) {
            s.set(ox, oy + i, oz, block, 10);
        }
    }
    true
}

/// `Biome::getBiomeType() == 15`（沼泽及其变种）
fn is_swamp_type(id: u8) -> bool {
    id == 6 || id == 134
}

/// `VineBlock::_isAcceptableNeighbor`
fn vine_neighbor_ok(id: u8) -> bool {
    id != AIR
        && (bp::shape(id) == 0 || bp::props(id) & bp::PROP_SOLID_BLOCKING != 0)
        && bp::blocks_motion(id)
        && id != INVISIBLE_BEDROCK
}

/// `VinesFeature::place`
pub fn vines(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    trace("VinesFeature", pos, r);
    const FACING_DIRECTION: [i32; 6] = [255, 255, 2, 0, 1, 3];
    const OPPOSITE: [usize; 6] = [1, 0, 3, 2, 5, 4];
    let (mut x, mut y, mut z) = pos;
    while y <= 127 {
        if s.is_empty(x, y, z) {
            for face in 2..6usize {
                let n = match face {
                    2 => s.get(x, y, z + 1),
                    3 => s.get(x, y, z - 1),
                    4 => s.get(x + 1, y, z),
                    _ => s.get(x - 1, y, z),
                };
                if vine_neighbor_ok(n) {
                    let dir = FACING_DIRECTION[OPPOSITE[face]];
                    s.set(x, y, z, VINE, (1 << dir) as u8);
                    break;
                }
            }
        } else {
            x = pos.0 + r.next_int(4) as i32 - r.next_int(4) as i32;
            z = pos.2 + r.next_int(4) as i32 - r.next_int(4) as i32;
        }
        y += 1;
    }
}

// ---------------------------------------------------------------- 树

/// 装饰器里的各种树
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tree {
    /// OakFeature(false, 0, 0, false)：高 4..6
    Oak,
    /// BirchFeature(false, tall)
    Birch { tall: bool },
    /// SpruceFeature
    Spruce,
    /// PineFeature
    Pine,
    /// RoofTreeFeature
    Roof,
    /// MegaPineTreeFeature(false, isSpruce)
    MegaPine { spruce: bool },
    /// FancyTreeFeature
    Fancy,
    /// SwampTreeFeature
    Swamp,
    /// GroundBushFeature(3, 3)
    GroundBush,
    /// JungleTreeFeature
    Jungle,
    /// MegaJungleTreeFeature(false, 10, 20, 3, 3)
    MegaJungle,
    /// SavannaTreeFeature
    Savanna,
    /// HugeMushroomFeature（沼泽的"树"）
    HugeMushroom,
}

pub fn place_tree(s: &mut Src, tree: Tree, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let name = match tree {
        Tree::Oak => "OakFeature",
        Tree::Birch { .. } => "BirchFeature",
        Tree::Spruce => "SpruceFeature",
        Tree::Pine => "PineFeature",
        Tree::Roof => "RoofTreeFeature",
        Tree::MegaPine { .. } => "MegaPineTreeFeature",
        Tree::Fancy => "FancyTreeFeature",
        Tree::Swamp => "SwampTreeFeature",
        Tree::GroundBush => "GroundBushFeature",
        Tree::Jungle => "JungleTreeFeature",
        Tree::MegaJungle => "MegaJungleTreeFeature",
        Tree::Savanna => "SavannaTreeFeature",
        Tree::HugeMushroom => "",
    };
    if !name.is_empty() {
        trace(name, pos, r);
    }
    match tree {
        Tree::Oak => {
            let h = r.next_int(3) as i32 + 4;
            TreeBase::new(0, 0, false).place(s, pos, r, h)
        }
        Tree::Birch { tall } => {
            let mut h = r.next_int(3) as i32 + 5;
            if tall {
                h += r.next_int(7) as i32;
            }
            TreeBase::new(2, 2, false).place(s, pos, r, h)
        }
        Tree::Jungle => {
            let h = r.next_int(7) as i32 + 4;
            TreeBase::new(3, 3, true).place(s, pos, r, h)
        }
        Tree::Spruce => spruce(s, pos, r),
        Tree::Pine => pine(s, pos, r),
        Tree::Roof => roof_tree(s, pos, r),
        Tree::MegaPine { spruce } => mega_pine(s, pos, r, spruce),
        Tree::Fancy => fancy_tree(s, pos, r),
        Tree::Swamp => swamp_tree(s, pos, r),
        Tree::GroundBush => ground_bush(s, pos, r),
        Tree::MegaJungle => mega_jungle(s, pos, r),
        Tree::Savanna => savanna_tree(s, pos, r),
        Tree::HugeMushroom => huge_mushroom(s, pos, r),
    }
}

/// `TreeFeature::_isFree(const Material&)`
#[inline]
fn free_mat(m: u8) -> bool {
    m == mat::AIR || m == mat::LEAVES
}
/// `TreeFeature::_isFree(BlockID)`
#[inline]
fn free_id(id: u8) -> bool {
    free_mat(bp::material(id)) || id == GRASS || id == DIRT
}

struct TreeBase {
    trunk: u8,
    leaf: u8,
    jungle: bool,
}

const LOG_AXIS: [u8; 6] = [0, 0, 8, 8, 4, 4];
const FACING_DIR: [(i32, i32, i32); 6] = [(0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)];
const STEP_X: [i32; 4] = [0, -1, 0, 1];
const STEP_Z: [i32; 4] = [1, 0, -1, 0];
const DIRECTION_OPPOSITE: [usize; 4] = [2, 3, 0, 1];

impl TreeBase {
    fn new(trunk: u8, leaf: u8, jungle: bool) -> Self {
        Self { trunk, leaf, jungle }
    }

    fn place_leaf(&self, s: &mut Src, x: i32, y: i32, z: i32) {
        let id = s.get(x, y, z);
        if !bp::msolid(id) {
            s.set(x, y, z, LEAVES, self.leaf);
        }
    }

    fn add_vine(&self, s: &mut Src, x: i32, y: i32, z: i32, data: u8) {
        s.set(x, y, z, VINE, data);
        let mut cy = y - 1;
        let mut count = 5;
        while s.get(x, cy, z) == AIR {
            count -= 1;
            if count == 0 {
                break;
            }
            s.set(x, cy, z, VINE, data);
            cy -= 1;
        }
    }

    fn add_jungle_features(&self, s: &mut Src, x: i32, y: i32, z: i32, r: &mut McpeRandom) {
        if s.get(x, y, z) != LEAVES {
            return;
        }
        if r.next_int(4) == 0 && s.get(x - 1, y, z) == AIR {
            self.add_vine(s, x - 1, y, z, VINE_EAST);
        }
        if r.next_int(4) == 0 && s.get(x + 1, y, z) == AIR {
            self.add_vine(s, x + 1, y, z, VINE_WEST);
        }
        if r.next_int(4) == 0 && s.get(x, y, z - 1) == AIR {
            self.add_vine(s, x, y, z - 1, VINE_SOUTH);
        }
        if r.next_int(4) == 0 && s.get(x, y, z + 1) == AIR {
            self.add_vine(s, x, y, z + 1, VINE_NORTH);
        }
    }

    fn add_cocoa(&self, s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, height: i32) {
        let mut i = 4;
        while i != 2 {
            for j in 0..4usize {
                if r.next_int(i as u32) == 0 {
                    let age = r.next_int(3) as u8;
                    let dir = DIRECTION_OPPOSITE[j];
                    s.set(
                        pos.0 + STEP_X[dir],
                        pos.1 + (height - 1 - i),
                        pos.2 + STEP_Z[dir],
                        COCOA,
                        (age << 2) | j as u8,
                    );
                }
            }
            i -= 1;
        }
    }

    /// `TreeFeature::_prepareSpawn`
    fn prepare_spawn(&self, s: &mut Src, pos: (i32, i32, i32), height: i32) -> bool {
        let (px, py, pz) = pos;
        if !(py > 0 && py + height <= 127) {
            return false;
        }
        if !bush_soil(s.get(px, py - 1, pz)) {
            return false;
        }
        let mut free_all = true;
        for y in py..=py + height + 1 {
            let rr = if height + py - 1 > y {
                if y != py {
                    1
                } else {
                    0
                }
            } else {
                2
            };
            let mut x = px - rr;
            while x <= px + rr && free_all {
                let mut z = pz - rr;
                while z <= pz + rr && free_all {
                    free_all = if (0..128).contains(&y) { free_id(s.get(x, y, z)) } else { false };
                    z += 1;
                }
                x += 1;
            }
        }
        if free_all {
            s.set(px, py - 1, pz, DIRT, 0);
            true
        } else {
            false
        }
    }

    /// `TreeFeature::_placeTrunk`
    fn place_trunk(&self, s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, height: i32) -> bool {
        let mut result = false;
        let vine_chance: f32;
        let mut trunk_height = height;
        if r.next_int(80) == 0 {
            vine_chance = if self.trunk == 2 { 0.0 } else { 0.75 };
            trunk_height = 1;
        } else {
            result = true;
            vine_chance = if self.trunk == 2 {
                0.0
            } else if self.jungle {
                0.33333334
            } else if r.next_int(12) == 0 {
                1.0
            } else {
                0.0
            };
        }
        if !result {
            self.place_fallen_trunk(s, pos, r, height);
        }
        for i in 0..trunk_height {
            let (x, y, z) = (pos.0, pos.1 + i, pos.2);
            let id = s.get(x, y, z);
            if id == AIR || id == LEAVES {
                s.set(x, y, z, LOG, self.trunk);
                if vine_chance != 0.0 {
                    if r.next_float() < vine_chance && s.is_empty(x - 1, y, z) {
                        s.set(x - 1, y, z, VINE, VINE_EAST);
                    }
                    if r.next_float() < vine_chance && s.is_empty(x + 1, y, z) {
                        s.set(x + 1, y, z, VINE, VINE_WEST);
                    }
                    if r.next_float() < vine_chance && s.is_empty(x, y, z - 1) {
                        s.set(x, y, z - 1, VINE, VINE_SOUTH);
                    }
                    if r.next_float() < vine_chance && s.is_empty(x, y, z + 1) {
                        s.set(x, y, z + 1, VINE, VINE_NORTH);
                    }
                }
            }
        }
        result
    }

    /// `TreeFeature::_placeFallenTrunk`
    fn place_fallen_trunk(&self, s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, height: i32) {
        let dir_index = r.next_int(4) as usize + 2;
        let dir = FACING_DIR[dir_index];
        let length = r.next_int(2) as i32 + 2;
        let mut start = (pos.0 + dir.0 * length, pos.1 + dir.1 * length, pos.2 + dir.2 * length);
        start.1 = s.heightmap(start.0, start.2);
        if start.1 > pos.1 + 1 {
            return;
        }
        let mut cur = start;
        let mut gap = 0;
        for _ in 0..height - 2 {
            if !s.is_empty(cur.0, cur.1, cur.2) {
                return;
            }
            gap += 1;
            if s.solid_blocking_block(cur.0, cur.1 - 1, cur.2) {
                gap = 0;
            } else if gap > 2 {
                return;
            }
            cur = (cur.0 + dir.0, cur.1 + dir.1, cur.2 + dir.2);
        }
        cur = start;
        for _ in 0..height - 2 {
            s.set(cur.0, cur.1, cur.2, LOG, LOG_AXIS[dir_index] | self.trunk);
            if r.next_int(10) == 0 && s.is_empty(cur.0, cur.1 + 1, cur.2) {
                let id = if r.next_float() < 0.5 { BROWN_MUSHROOM } else { RED_MUSHROOM };
                s.set(cur.0, cur.1 + 1, cur.2, id, 0);
            }
            cur = (cur.0 + dir.0, cur.1 + dir.1, cur.2 + dir.2);
        }
    }

    /// `TreeFeature::place(source, pos, random, height)`
    fn place(&self, s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, height: i32) -> bool {
        if !self.prepare_spawn(s, pos, height) {
            return false;
        }
        if !self.place_trunk(s, pos, r, height) {
            return true;
        }
        let (px, py, pz) = pos;
        for y in py - 3 + height..=py + height {
            let level = y - (py + height);
            let rr = 1 - level / 2;
            for x in px - rr..=px + rr {
                let dx = x - px;
                for z in pz - rr..=pz + rr {
                    let dz = z - pz;
                    if dx.abs() == rr && dz.abs() == rr && (r.next_int(2) == 0 || level == 0) {
                        continue;
                    }
                    self.place_leaf(s, x, y, z);
                }
            }
        }
        if self.jungle {
            for y in py - 3 + height..=py + height {
                let level = y - (py + height);
                let rr = 2 - level / 2;
                for x in px - rr..=px + rr {
                    for z in pz - rr..=pz + rr {
                        self.add_jungle_features(s, x, y, z, r);
                    }
                }
            }
            if r.next_int(5) == 0 && height > 5 {
                self.add_cocoa(s, pos, r, height);
            }
        }
        s.set(px, py - 1, pz, DIRT, 0);
        true
    }
}

/// `SpruceFeature::place`
fn spruce(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let base = TreeBase::new(1, 1, false);
    let height = r.next_int(4) as i32 + 6;
    if !base.prepare_spawn(s, pos, height) {
        return false;
    }
    let crown = r.next_int(2) as i32 + 1;
    let cap = r.next_int(2) as i32 + 2;
    let th = height - r.next_int(3) as i32;
    if !base.place_trunk(s, pos, r, th) {
        return true;
    }
    let mut radius = r.next_int(2) as i32;
    let mut state = 1;
    let mut reset = 0;
    let last = height - crown;
    for layer in 0..=last {
        let y = pos.1 + height - layer;
        for x in pos.0 - radius..=pos.0 + radius {
            let dx = x - pos.0;
            for z in pos.2 - radius..=pos.2 + radius {
                let dz = z - pos.2;
                if dx.abs() == radius && dz.abs() == radius && radius > 0 {
                    continue;
                }
                base.place_leaf(s, x, y, z);
            }
        }
        if radius >= state {
            radius = reset;
            reset = 1;
            state += 1;
            if state > cap {
                state = cap;
            }
        } else {
            radius += 1;
        }
    }
    true
}

/// `PineFeature::place`
fn pine(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let v59 = r.next_int(5) as i32;
    let v51 = v59 + 7;
    let v6 = r.next_int(2) as i32;
    let v58 = v51 - v6 - 3;
    let v57 = r.next_int((v6 + 4) as u32) as i32 + 1;
    let (x0, y0, z0) = pos;
    if y0 <= 0 || y0 + v51 > 127 {
        return false;
    }
    let mut y = y0;
    loop {
        let rr = if v58 > y - y0 { 0 } else { v57 };
        for x in x0 - rr..=x0 + rr {
            if y > 127 {
                return false;
            }
            for z in z0 - rr..=z0 + rr {
                let id = s.get(x, y, z);
                if id != AIR && id != LEAVES {
                    return false;
                }
            }
        }
        if y0 + 1 + v51 < y + 1 {
            break;
        }
        y += 1;
    }
    let below = s.get(x0, y0 - 1, z0);
    if below != GRASS && below != DIRT {
        return false;
    }
    if y0 >= 120 - v59 {
        return false;
    }
    s.set(x0, y0 - 1, z0, DIRT, 0);
    let v60 = y0 + v58 - 1;
    let mut ycur = y0 + v51;
    let mut r5 = 0;
    loop {
        for x in x0 - r5..=x0 + r5 {
            for z in z0 - r5..=z0 + r5 {
                if r5 != 0 && (x - x0).abs() == r5 && (z - z0).abs() == r5 {
                    continue;
                }
                let id = s.get(x, ycur, z);
                if !bp::msolid(id) {
                    s.set(x, ycur, z, LEAVES, 1);
                }
            }
        }
        if r5 == 0 {
            r5 += 1;
        } else if y0 + v58 + 1 == ycur {
            r5 -= 1;
        } else if r5 < v57 {
            r5 += 1;
        }
        ycur -= 1;
        if ycur == v60 {
            break;
        }
    }
    let mut i = 0;
    loop {
        let id = s.get(x0, y0 + i, z0);
        if id == AIR || id == LEAVES {
            s.set(x0, y0 + i, z0, LOG, 1);
        }
        if i + 1 == v59 + 6 {
            break;
        }
        i += 1;
    }
    true
}

/// `SwampTreeFeature::place`
fn swamp_tree(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let (ox, mut oy, oz) = pos;
    let off = r.next_int(4) as i32;
    let height = off + 5;
    while s.mat(ox, oy - 1, oz) == mat::WATER || s.get(ox, oy - 1, oz) == LOG {
        oy -= 1;
    }
    let ground_y = oy - 1;
    if oy <= 0 || height + oy > 127 {
        return false;
    }
    let mut free = true;
    for level in oy..=oy + 1 + height {
        let radius = if level == oy {
            0
        } else if level >= oy + 1 + height - 2 {
            3
        } else {
            1
        };
        let mut bx = ox - radius;
        while bx <= ox + radius && free {
            let mut bz = oz - radius;
            while bz <= oz + radius && free {
                if (0..128).contains(&level) {
                    let id = s.get(bx, level, bz);
                    let m = bp::material(id);
                    if id != LOG && !free_mat(m) {
                        if m != mat::WATER || level > oy {
                            free = false;
                        }
                    }
                } else {
                    free = false;
                }
                bz += 1;
            }
            bx += 1;
        }
    }
    if !free {
        return false;
    }
    let ground = s.get(ox, ground_y, oz);
    if (ground != GRASS && ground != DIRT) || oy >= 122 - off {
        return false;
    }
    s.set(ox, ground_y, oz, DIRT, 0);
    for level in oy - 3 + height..=oy + height {
        let rel = level - (oy + height);
        let radius = 2 - rel / 2;
        for bx in ox - radius..=ox + radius {
            let dx = bx - ox;
            for bz in oz - radius..=oz + radius {
                let dz = bz - oz;
                if dx.abs() == radius && dz.abs() == radius && (r.next_int(2) == 0 || rel == 0) {
                    continue;
                }
                if !s.solid_blocking_block(bx, level, bz) {
                    s.set(bx, level, bz, LEAVES, 0);
                }
            }
        }
    }
    for i in 0..height {
        let m = s.mat(ox, oy + i, oz);
        if free_mat(m) || m == mat::WATER {
            s.set(ox, oy + i, oz, LOG, 0);
        }
    }
    let add_vine = |s: &mut Src, x: i32, y: i32, z: i32, data: u8| {
        s.set(x, y, z, VINE, data);
        let mut yy = y - 1;
        let mut remaining = 5;
        while s.mat(x, yy, z) == mat::AIR && {
            remaining -= 1;
            remaining > 0
        } {
            s.set(x, yy, z, VINE, data);
            yy -= 1;
        }
    };
    for level in oy - 3 + height..=oy + height {
        let rel = level - (oy + height);
        let radius = 2 - rel / 2;
        for bx in ox - radius..=ox + radius {
            for bz in oz - radius..=oz + radius {
                if s.mat(bx, level, bz) == mat::LEAVES {
                    if r.next_int(4) == 0 && s.mat(bx - 1, level, bz) == mat::AIR {
                        add_vine(s, bx - 1, level, bz, VINE_EAST);
                    }
                    if r.next_int(4) == 0 && s.mat(bx + 1, level, bz) == mat::AIR {
                        add_vine(s, bx + 1, level, bz, VINE_WEST);
                    }
                    if r.next_int(4) == 0 && s.mat(bx, level, bz - 1) == mat::AIR {
                        add_vine(s, bx, level, bz - 1, VINE_SOUTH);
                    }
                    if r.next_int(4) == 0 && s.mat(bx, level, bz + 1) == mat::AIR {
                        add_vine(s, bx, level, bz + 1, VINE_NORTH);
                    }
                }
            }
        }
    }
    true
}

/// `GroundBushFeature::place`
fn ground_bush(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let (px, mut py, pz) = pos;
    while free_mat(s.mat(px, py, pz)) && py > 0 {
        py -= 1;
    }
    let ground = s.get(px, py, pz);
    if std::env::var("TRACE2").is_ok() {
        println!("BUSH ground {ground} at y {py} start {:?} col {:?}", pos, (55..70).map(|y| s.get(px, y, pz)).collect::<Vec<_>>());
    }
    if ground == DIRT || ground == GRASS {
        s.set(px, py, pz, LOG, 3);
        for y in py..py + 3 {
            let yo = y - py;
            let offs = 2 - yo;
            for x in px - offs..px + offs + 1 {
                let xo = x - px;
                for z in pz - offs..pz + offs + 1 {
                    let zo = z - pz;
                    if xo.abs() != offs || zo.abs() != offs || r.next_int(2) != 0 {
                        if !bp::block_is_solid(s.get(x, y, z)) {
                            s.set(x, y, z, LEAVES, 3);
                        }
                    }
                }
            }
        }
    }
    true
}

/// `SavannaTreeFeature::place`
fn savanna_tree(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let base = TreeBase::new(0, 0, false);
    let a = r.next_int(3) as i32;
    let b = r.next_int(3) as i32;
    let height = a + b + 5;
    if !base.prepare_spawn(s, pos, height) || pos.1 >= 128 - height - 1 {
        return false;
    }
    let leaf = |s: &mut Src, x: i32, y: i32, z: i32| {
        if free_mat(s.mat(x, y, z)) {
            s.set(x, y, z, LEAVES2, 0);
        }
    };
    s.set(pos.0, pos.1 - 1, pos.2, DIRT, 0);
    let dir = r.next_int(4) as usize;
    let bend_start = height - r.next_int(4) as i32 - 1;
    let mut bend_count = 3 - r.next_int(3) as i32;
    let (mut tx, mut ty, mut tz) = pos;
    let mut top = 0;
    for i in 0..height {
        let y = pos.1 + i;
        if i >= bend_start && bend_count > 0 {
            tx += STEP_X[dir];
            tz += STEP_Z[dir];
            bend_count -= 1;
        }
        ty = y;
        if free_mat(s.mat(tx, ty, tz)) {
            top = y;
            s.set(tx, ty, tz, LOG2, 0);
        }
    }
    let _ = ty;
    ty = top;
    for x in -1..=1 {
        for z in -1..=1 {
            leaf(s, tx + x, ty + 1, tz + z);
        }
    }
    leaf(s, tx + 2, ty + 1, tz);
    leaf(s, tx - 2, ty + 1, tz);
    leaf(s, tx, ty + 1, tz + 2);
    leaf(s, tx, ty + 1, tz - 2);
    for x in -3i32..=3 {
        for z in -3i32..=3 {
            if x.abs() == 3 && z.abs() == 3 {
                continue;
            }
            leaf(s, tx + x, ty, tz + z);
        }
    }
    tx = pos.0;
    tz = pos.2;
    let dir2 = r.next_int(4) as usize;
    if dir2 != dir {
        let start = bend_start - r.next_int(2) as i32 - 1;
        let mut count = 1 + r.next_int(3) as i32;
        top = 0;
        let mut i = start;
        while i < height && count > 0 {
            if i >= 1 {
                let y = pos.1 + i;
                tx += STEP_X[dir2];
                tz += STEP_Z[dir2];
                ty = y;
                if free_mat(s.mat(tx, ty, tz)) {
                    top = y;
                    s.set(tx, ty, tz, LOG2, 0);
                }
            }
            i += 1;
            count -= 1;
        }
        if top > 0 {
            ty = top;
            for x in -1..=1 {
                for z in -1..=1 {
                    leaf(s, tx + x, ty + 1, tz + z);
                }
            }
            for x in -2i32..=2 {
                for z in -2i32..=2 {
                    if x.abs() == 2 && z.abs() == 2 {
                        continue;
                    }
                    leaf(s, tx + x, ty, tz + z);
                }
            }
        }
    }
    true
}

/// `RoofTreeFeature::place`
fn roof_tree(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let base = TreeBase::new(1, 1, false);
    let a = r.next_int(3) as i32;
    let b = r.next_int(2) as i32;
    let height = a + b + 6;
    let mut placed = base.prepare_spawn(s, pos, height);
    if placed && pos.1 >= 128 - height - 1 {
        placed = false;
    }
    if !placed {
        return false;
    }
    let (px, py, pz) = pos;
    for (x, z) in [(px, pz), (px + 1, pz), (px + 1, pz + 1), (px, pz + 1)] {
        s.set(x, py - 1, z, DIRT, 0);
    }
    let dir = r.next_int(4) as usize;
    let bend_start = height - r.next_int(4) as i32;
    let mut bend_count = 2 - r.next_int(3) as i32;
    let (mut tx, mut tz) = (px, pz);
    let vines = r.next_int(6) == 0;
    let mut top = 0;
    let place_vine = |s: &mut Src, x: i32, y: i32, z: i32, r: &mut McpeRandom, data: u8| {
        if r.next_int(7) > 0 && s.is_empty(x, y, z) {
            s.set(x, y, z, VINE, data);
        }
    };
    let place_vines = |s: &mut Src, x: i32, y: i32, z: i32, r: &mut McpeRandom, dx: u8, dz: u8| {
        if free_mat(s.mat(x, y, z)) {
            s.set(x, y, z, LOG2, 1);
            if vines {
                let side = if dx == VINE_EAST { (x - 1, z) } else { (x + 1, z) };
                let front = if dz == VINE_NORTH { (x, z + 1) } else { (x, z - 1) };
                place_vine(s, side.0, y, side.1, r, dx);
                place_vine(s, front.0, y, front.1, r, dz);
            }
        }
    };
    for i in 0..height {
        let y = py + i;
        if i >= bend_start && bend_count > 0 {
            tx += STEP_X[dir];
            tz += STEP_Z[dir];
            bend_count -= 1;
        }
        if free_mat(s.mat(tx, y, tz)) {
            top = y;
            place_vines(s, px, y, pz, r, VINE_EAST, VINE_SOUTH);
            place_vines(s, px + 1, y, pz, r, VINE_WEST, VINE_SOUTH);
            place_vines(s, px + 1, y, pz + 1, r, VINE_WEST, VINE_NORTH);
            place_vines(s, px, y, pz + 1, r, VINE_EAST, VINE_NORTH);
        }
    }
    let t = (tx, top, tz);
    let leaf = |s: &mut Src, x: i32, y: i32, z: i32| {
        let m = s.mat(x, y, z);
        if m == mat::AIR || m == 24 {
            s.set(x, y, z, LEAVES2, 1);
        }
    };
    for x in -2..1 {
        for z in -2..1 {
            leaf(s, t.0 + x, t.1 - 1, t.2 + z);
            leaf(s, t.0 + 1 - x, t.1 - 1, t.2 + z);
            leaf(s, t.0 + x, t.1 - 1, t.2 + 1 - z);
            leaf(s, t.0 + 1 - x, t.1 - 1, t.2 + 1 - z);
            if (x < -1 && z < 0) || (x == -1 && z == -2) {
                continue;
            }
            leaf(s, t.0 + x, t.1 + 1, t.2 + z);
            leaf(s, t.0 + 1 - x, t.1 + 1, t.2 + z);
            leaf(s, t.0 + x, t.1 + 1, t.2 + 1 - z);
            leaf(s, t.0 + 1 - x, t.1 + 1, t.2 + 1 - z);
        }
    }
    if r.next_boolean() {
        leaf(s, t.0, t.1 + 2, t.2);
        leaf(s, t.0 + 1, t.1 + 2, t.2);
        leaf(s, t.0 + 1, t.1 + 2, t.2 + 1);
        leaf(s, t.0, t.1 + 2, t.2 + 1);
    }
    for x in -3i32..5 {
        for z in -3i32..5 {
            if (x == -3 && z == -3)
                || (x == -3 && z == 4)
                || (x == 4 && z == -3)
                || (x == 4 && z == 4)
                || (x.abs() >= 3 && z.abs() >= 3)
            {
                continue;
            }
            leaf(s, t.0 + x, t.1, t.2 + z);
        }
    }
    for x in -1..3 {
        for z in -1..3 {
            if (0..=1).contains(&x) && (0..=1).contains(&z) {
                continue;
            }
            if r.next_int(3) > 0 {
                continue;
            }
            let length = r.next_int(3) as i32 + 2;
            for i in 0..length {
                s.set(px + x, t.1 - i - 1, pz + z, LOG2, 1);
            }
            for lx in -1..2 {
                for lz in -1..2 {
                    leaf(s, t.0 + x + lx, t.1, t.2 + z + lz);
                }
            }
            for lx in -2i32..3 {
                for lz in -2i32..3 {
                    if lx.abs() == 2 && lz.abs() == 2 {
                        continue;
                    }
                    leaf(s, t.0 + x + lx, t.1 - 1, t.2 + z + lz);
                }
            }
        }
    }
    true
}

// --- MegaTreeFeature

struct Mega {
    base_height: i32,
    interval: i32,
    trunk: u8,
    leaf: u8,
}

impl Mega {
    fn calc_height(&self, r: &mut McpeRandom) -> i32 {
        let mut h = r.next_int(3) as i32 + self.base_height;
        if self.interval > 1 {
            h += r.next_int(self.interval as u32) as i32;
        }
        h
    }

    fn check_free(&self, s: &Src, pos: (i32, i32, i32), height: i32) -> bool {
        let mut free = true;
        if pos.1 < 1 || pos.1 + height + 1 > 128 {
            return false;
        }
        for yo in 0..height + 2 {
            let radius = if yo == 0 { 1 } else { 2 };
            let mut xo = -radius;
            while xo <= radius && free {
                let mut zo = -radius;
                while zo <= radius && free {
                    let y = pos.1 + yo;
                    if !(0..128).contains(&y) || !free_id(s.get(pos.0 + xo, y, pos.2 + zo)) {
                        free = false;
                    }
                    zo += 1;
                }
                xo += 1;
            }
        }
        free
    }

    fn make_dirt_floor(&self, s: &mut Src, pos: (i32, i32, i32)) -> bool {
        let (bx, by, bz) = (pos.0, pos.1 - 1, pos.2);
        let id = s.get(bx, by, bz);
        if (id == GRASS || id == DIRT || id == PODZOL) && pos.1 >= 2 {
            s.set(bx, by, bz, DIRT, 0);
            s.set(bx + 1, by, bz, DIRT, 0);
            s.set(bx, by, bz + 1, DIRT, 0);
            s.set(bx + 1, by, bz + 1, DIRT, 0);
            return true;
        }
        false
    }

    fn prepare(&self, s: &mut Src, pos: (i32, i32, i32), height: i32) -> bool {
        if !self.check_free(s, pos, height) {
            return false;
        }
        self.make_dirt_floor(s, pos)
    }

    fn double_trunk_leaves(&self, s: &mut Src, pos: (i32, i32, i32), level: i32) {
        let rsq = level * level;
        for dx in -level..level + 2 {
            let k = dx - 1;
            for dz in -level..level + 2 {
                let l = dz - 1;
                if dz * dz + dx * dx > rsq && l * l + k * k > rsq && l * l + dx * dx > rsq && dz * dz + k * k > rsq {
                    continue;
                }
                let (x, y, z) = (pos.0 + dx, pos.1, pos.2 + dz);
                let id = s.get(x, y, z);
                let d = s.data(x, y, z);
                if (id != LEAVES || d != self.leaf) && !free_mat(bp::material(id)) {
                    continue;
                }
                s.set(x, y, z, LEAVES, self.leaf);
            }
        }
    }

    fn single_trunk_leaves(&self, s: &mut Src, pos: (i32, i32, i32), level: i32) {
        let rsq = level * level;
        for dx in -level..level + 1 {
            for dz in -level..level + 1 {
                if rsq < dx * dx + dz * dz {
                    continue;
                }
                let (x, y, z) = (pos.0 + dx, pos.1, pos.2 + dz);
                if !free_mat(s.mat(x, y, z)) {
                    continue;
                }
                s.set(x, y, z, LEAVES, self.leaf);
            }
        }
    }
}

/// `MegaPineTreeFeature::place`
fn mega_pine(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom, is_spruce: bool) -> bool {
    let m = Mega { base_height: 13, interval: 15, trunk: 1, leaf: 1 };
    let height = m.calc_height(r);
    if !m.prepare(s, pos, height) {
        return false;
    }
    // createCrown
    {
        let top = pos.1 + height;
        let extra = r.next_int(5) as i32;
        let h = if is_spruce { m.base_height } else { 3 } + extra;
        let mut prev = 0;
        let mut y = top - h;
        while y != top + 1 {
            let dy = top - y;
            let width = ((dy as f32 / h as f32 * 3.5f32).floor()) as i32;
            let bump = if dy > 0 && width == prev && (y & 1) == 0 { 1 } else { 0 };
            m.double_trunk_leaves(s, (pos.0, y, pos.2), width + bump);
            prev = width;
            y += 1;
        }
    }
    let log = |s: &mut Src, x: i32, y: i32, z: i32| {
        if free_mat(s.mat(x, y, z)) {
            s.set(x, y, z, LOG, m.trunk);
        }
    };
    for i in 0..height {
        log(s, pos.0, pos.1 + i, pos.2);
        if i < height - 1 {
            log(s, pos.0 + 1, pos.1 + i, pos.2);
            log(s, pos.0 + 1, pos.1 + i, pos.2 + 1);
            log(s, pos.0, pos.1 + i, pos.2 + 1);
        }
    }
    true
}

/// `MegaJungleTreeFeature::place`
fn mega_jungle(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let m = Mega { base_height: 10, interval: 20, trunk: 3, leaf: 3 };
    let height = m.calc_height(r);
    if !m.prepare(s, pos, height) {
        return false;
    }
    for dy in -2..=0 {
        m.double_trunk_leaves(s, (pos.0, pos.1 + height + dy, pos.2), 3 - dy);
    }
    let mut y = pos.1 + height - 2 - r.next_int(4) as i32;
    while pos.1 + height / 2 < y {
        let angle = r.next_float() * PI * 2.0;
        let mut tx = pos.0 + (0.5f32 + mcos(angle) * 4.0) as i32;
        let mut tz = pos.2 + (0.5f32 + msin(angle) * 4.0) as i32;
        for step in 0..5 {
            tx = pos.0 + (1.5f32 + mcos(angle) * step as f32) as i32;
            tz = pos.2 + (1.5f32 + msin(angle) * step as f32) as i32;
            s.set(tx, y - 3 + step / 2, tz, LOG, m.trunk);
        }
        let layers = 1 + r.next_int(2) as i32;
        for ly in y - layers..=y {
            m.single_trunk_leaves(s, (tx, ly, tz), 1 - (ly - y));
        }
        y -= 2 + r.next_int(4) as i32;
    }
    let place_vine = |s: &mut Src, x: i32, y: i32, z: i32, r: &mut McpeRandom, data: u8| {
        if r.next_int(3) > 0 && s.is_empty(x, y, z) {
            s.set(x, y, z, VINE, data);
        }
    };
    let place_vines = |s: &mut Src, x: i32, y: i32, z: i32, r: &mut McpeRandom, level: i32, fa: u8, fb: u8| {
        if !free_mat(s.mat(x, y, z)) {
            return;
        }
        s.set(x, y, z, LOG, 3);
        if level <= 0 {
            return;
        }
        let side = if fa == VINE_EAST { (x - 1, z) } else { (x + 1, z) };
        let other = if fb == VINE_NORTH { (x, z + 1) } else { (x, z - 1) };
        place_vine(s, side.0, y, side.1, r, fa);
        place_vine(s, other.0, y, other.1, r, fb);
    };
    for level in 0..height {
        let (x, y, z) = (pos.0, pos.1 + level, pos.2);
        place_vines(s, x, y, z, r, level, VINE_EAST, VINE_SOUTH);
        if level < height - 1 {
            place_vines(s, x + 1, y, z, r, level, VINE_WEST, VINE_SOUTH);
            place_vines(s, x + 1, y, z + 1, r, level, VINE_WEST, VINE_NORTH);
            place_vines(s, x, y, z + 1, r, level, VINE_EAST, VINE_NORTH);
        }
    }
    true
}

// --- FancyTreeFeature

fn fancy_steps(d: (i32, i32, i32)) -> i32 {
    let (x, y, z) = (d.0.abs(), d.1.abs(), d.2.abs());
    if x < z && y < z {
        z
    } else if y < x {
        x
    } else {
        y
    }
}

fn fancy_check_line(s: &Src, from: (i32, i32, i32), to: (i32, i32, i32)) -> i32 {
    let d = (to.0 - from.0, to.1 - from.1, to.2 - from.2);
    let steps = fancy_steps(d);
    let inv = 1.0f32 / steps as f32;
    let (fx, fy, fz) = (inv * d.0 as f32, inv * d.1 as f32, inv * d.2 as f32);
    for i in 0..steps + 1 {
        let x = from.0 + (0.5f32 + i as f32 * fx) as i32;
        let y = from.1 + (0.5f32 + i as f32 * fy) as i32;
        let z = from.2 + (0.5f32 + i as f32 * fz) as i32;
        if !free_id(s.get(x, y, z)) {
            return i;
        }
    }
    -1
}

fn fancy_limb(s: &mut Src, from: (i32, i32, i32), to: (i32, i32, i32)) {
    let d = (to.0 - from.0, to.1 - from.1, to.2 - from.2);
    let steps = fancy_steps(d);
    let inv = 1.0f32 / steps as f32;
    let (fx, fy, fz) = (inv * d.0 as f32, inv * d.1 as f32, inv * d.2 as f32);
    for i in 0..steps + 1 {
        let at = (
            from.0 + (i as f32 * fx + 0.5f32) as i32,
            from.1 + (i as f32 * fy + 0.5f32) as i32,
            from.2 + (i as f32 * fz + 0.5f32) as i32,
        );
        let dx = (at.0 - from.0).abs();
        let dz = (at.2 - from.2).abs();
        let extent = dx.max(dz);
        let data = if extent > 0 {
            if dx == extent {
                4
            } else if dz == extent {
                8
            } else {
                0
            }
        } else {
            0
        };
        s.set(at.0, at.1, at.2, LOG, data);
    }
}

fn fancy_tree(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    const FOLIAGE_LAYERS: i32 = 4;
    let mut height = 5 + r.next_int(12) as i32;
    // _checkLocation
    let below = s.get(pos.0, pos.1 - 1, pos.2);
    if !(below == DIRT || below == GRASS || below == FARMLAND) {
        return false;
    }
    let found = fancy_check_line(s, pos, (pos.0, pos.1 + height - 1, pos.2));
    if found != -1 {
        if found < 6 {
            return false;
        }
        height = found;
    }
    let mut trunk_height = (height as f32 * 0.617999970912933349609375f32) as i32;
    if trunk_height >= height {
        trunk_height = height - 1;
    }
    let mut clusters = (1.382f32 + (1.0f32 * height as f32 / 13.0f32).powf(2.0)) as i32;
    if clusters < 1 {
        clusters = 1;
    }
    let trunk_top = pos.1 + trunk_height;
    let mut rel = height - FOLIAGE_LAYERS;
    let mut foliage: Vec<((i32, i32, i32), i32)> = vec![((pos.0, pos.1 + rel, pos.2), trunk_top)];
    while rel >= 0 {
        // _getTreeShape
        let shape = {
            let fh = height as f32;
            if (rel as f32) < fh * 0.3f32 {
                -1.0f32
            } else {
                let half = fh * 0.5f32;
                let diff = half - rel as f32;
                let mut radius = ((half * half - diff * diff) as f64).sqrt() as f32;
                if diff == 0.0 {
                    radius = half;
                    radius * 0.5
                } else if half <= diff.abs() {
                    0.0
                } else {
                    radius * 0.5
                }
            }
        };
        if shape >= 0.0 {
            for _ in 0..clusters {
                let radius = 1.0f32 * shape * (r.next_float() + 0.328f32);
                let angle = r.next_float() * 2.0f32 * PI;
                let x = (radius * msin(angle) + 0.5f32) as i32;
                let z = (radius * mcos(angle) + 0.5f32) as i32;
                let cs = (pos.0 + x, pos.1 + rel - 1, pos.2 + z);
                let ce = (cs.0, cs.1 + FOLIAGE_LAYERS, cs.2);
                if fancy_check_line(s, cs, ce) != -1 {
                    continue;
                }
                let dx = pos.0 - cs.0;
                let dz = pos.2 - cs.2;
                let distance = ((dx * dx + dz * dz) as f64).sqrt() as f32;
                let branch_height = cs.1 as f32 - distance * 0.381000012159347534f32;
                let branch_top = if branch_height > trunk_top as f32 { trunk_top } else { branch_height as i32 };
                let base = (pos.0, branch_top, pos.2);
                if fancy_check_line(s, base, cs) != -1 {
                    continue;
                }
                foliage.push((cs, base.1));
            }
        }
        rel -= 1;
    }
    // _makeFoliage
    for &(c, _) in &foliage {
        for layer in 0..FOLIAGE_LAYERS {
            let radius: f32 = if layer == 0 || layer == FOLIAGE_LAYERS - 1 { 2.0 } else { 3.0 };
            let rr = (radius + 0.618f32) as i32;
            for dx in -rr..rr + 1 {
                for dz in -rr..rr + 1 {
                    let fx = (dx.abs() as f32 + 0.5f32) as f64;
                    let fz = (dz.abs() as f32 + 0.5f32) as f64;
                    if fx * fx + fz * fz <= (radius * radius) as f64 {
                        let (x, y, z) = (c.0 + dx, c.1 + layer, c.2 + dz);
                        if free_mat(s.mat(x, y, z)) {
                            s.set(x, y, z, LEAVES, 0);
                        }
                    }
                }
            }
        }
    }
    // _makeTrunk（trunkMode 1）
    fancy_limb(s, pos, (pos.0, pos.1 + trunk_height, pos.2));
    // _makeBranches
    for &(c, base) in &foliage {
        let dy = base - pos.1;
        if (dy as f32) < height as f32 * 0.2f32 {
            continue;
        }
        fancy_limb(s, (pos.0, base, pos.2), c);
    }
    true
}
