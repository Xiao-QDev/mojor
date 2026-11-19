//! postProcess 期间的液体：`BlockTickingQueue` + `LiquidBlockDynamic` / `LiquidBlockStatic`
//! （MCPE 0.14.1 `BlockTickingQueue.cpp`、`block/Liquid*.cpp`）。
//!
//! 泉水在装饰时直接 tick 一次；`_fixWaterAlongEdges` 和流动产生的 tick 进队列，
//! postProcess 末尾 `tickAllPendingTicks` 以 instatick 方式一次跑完。

use super::blockprops::{self as bp, mat};
use super::features::{Src, AIR, COBBLE, FIRE, FLOWING_LAVA, FLOWING_WATER, OBSIDIAN, STILL_LAVA, STILL_WATER, STONE};
use crate::level::generator::mcpe_noise::McpeRandom;

#[derive(Clone, Copy)]
struct Entry {
    pos: (i32, i32, i32),
    id: u8,
    tick: u64,
}

/// `BlockTickingQueue`（std::greater 的二叉堆，按 libstdc++ 的 push/pop_heap 实现）
pub struct TickQueue {
    heap: Vec<Entry>,
    current: u64,
    insta: bool,
    /// 原版用时间做种子；只影响岩浆 tick 延迟的先后
    rng: McpeRandom,
}

impl Default for TickQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl TickQueue {
    pub fn new() -> Self {
        Self {
            heap: Vec::new(),
            current: 0,
            insta: false,
            rng: McpeRandom::new(0),
        }
    }

    pub fn is_instaticking(&self) -> bool {
        self.insta
    }

    fn push(&mut self, e: Entry) {
        self.heap.push(e);
        let mut hole = self.heap.len() - 1;
        while hole > 0 {
            let parent = (hole - 1) / 2;
            if self.heap[parent].tick > e.tick {
                self.heap[hole] = self.heap[parent];
                hole = parent;
            } else {
                break;
            }
        }
        self.heap[hole] = e;
    }

    fn pop(&mut self) -> Entry {
        let top = self.heap[0];
        let last = self.heap.len() - 1;
        let value = self.heap[last];
        self.heap[last] = top;
        // __adjust_heap(first, 0, last, value)
        let len = last;
        let mut hole = 0usize;
        let mut second = 0usize;
        while len > 0 && second < (len - 1) / 2 {
            second = 2 * (second + 1);
            if self.heap[second].tick > self.heap[second - 1].tick {
                second -= 1;
            }
            self.heap[hole] = self.heap[second];
            hole = second;
        }
        if len >= 2 && (len & 1) == 0 && second == (len - 2) / 2 {
            second = 2 * (second + 1);
            self.heap[hole] = self.heap[second - 1];
            hole = second - 1;
        }
        // __push_heap(first, hole, 0, value)
        while hole > 0 {
            let parent = (hole - 1) / 2;
            if self.heap[parent].tick > value.tick {
                self.heap[hole] = self.heap[parent];
                hole = parent;
            } else {
                break;
            }
        }
        if len > 0 {
            self.heap[hole] = value;
        }
        self.heap.pop();
        top
    }
}

/// `BlockTickingQueue::add`
pub fn queue_add(s: &mut Src, pos: (i32, i32, i32), id: u8, delay: i32, r: &mut McpeRandom) {
    if !s.has_chunks_at(pos.0 - 8, pos.2 - 8, pos.0 + 8, pos.2 + 8) {
        return;
    }
    if delay < 0 {
        if s.get(pos.0, pos.1, pos.2) == id && id != 0 {
            tick(s, pos, id, r);
        }
    } else {
        let t = s.queue.current.wrapping_add(delay as i64 as u64);
        s.queue.push(Entry { pos, id, tick: t });
    }
}

/// `BlockTickingQueue::tickAllPendingTicks`
pub fn tick_all(s: &mut Src) {
    let mut rng = std::mem::replace(&mut s.queue.rng, McpeRandom::new(0));
    while !s.queue.heap.is_empty() {
        // tickPendingTicks(source, Tick::MAX, INT_MAX, true)
        s.queue.insta = true;
        let count = s.queue.heap.len();
        let mut ticked = 0;
        while !s.queue.heap.is_empty() && ticked < count {
            let cur = s.queue.pop();
            s.queue.current = cur.tick;
            let p = cur.pos;
            if s.has_chunks_at(p.0 - 8, p.2 - 8, p.0 + 8, p.2 + 8) && s.get(p.0, p.1, p.2) == cur.id && cur.id != 0 {
                tick(s, p, cur.id, &mut rng);
            }
            ticked += 1;
        }
        s.queue.current = u64::MAX;
        s.queue.insta = false;
    }
    s.queue.rng = rng;
}

/// `Block::mBlocks[id]->tick(source, pos, random)`（只有液体会进这个队列）
pub fn tick(s: &mut Src, pos: (i32, i32, i32), id: u8, r: &mut McpeRandom) {
    match id {
        FLOWING_WATER | FLOWING_LAVA => dynamic_tick(s, pos, id, r),
        STILL_LAVA => {
            try_spread_fire(s, pos, r);
        }
        _ => {}
    }
}

fn liquid_mat(id: u8) -> u8 {
    if id == FLOWING_WATER || id == STILL_WATER {
        mat::WATER
    } else {
        mat::LAVA
    }
}

fn tick_delay(id: u8) -> i32 {
    if liquid_mat(id) == mat::WATER {
        5
    } else {
        30
    }
}

/// `LiquidBlock::getDepth`
fn depth(s: &Src, m: u8, x: i32, y: i32, z: i32) -> i32 {
    if s.mat(x, y, z) != m {
        return -1;
    }
    s.data(x, y, z) as i32
}

/// `LiquidBlock::trySpreadFire`
fn try_spread_fire(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    if s.queue.insta {
        return;
    }
    const OFF: [(i32, i32, i32); 6] = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)];
    for _ in 0..10 {
        let x = pos.0 + r.next_int(3) as i32 - 1;
        let y = pos.1 + r.next_int(3) as i32 - 1;
        let z = pos.2 + r.next_int(3) as i32 - 1;
        if bp::flammable(s.get(x, y, z)) {
            for o in OFF {
                if s.get(x + o.0, y + o.1, z + o.2) == AIR {
                    s.set(x + o.0, y + o.1, z + o.2, FIRE, 0);
                    return;
                }
            }
        }
    }
}

/// `LiquidBlock::solidify`
fn solidify(s: &mut Src, id: u8, pos: (i32, i32, i32), nb: (i32, i32, i32)) -> bool {
    if nb.1.wrapping_sub(pos.1) < 0 || liquid_mat(id) != mat::LAVA || s.mat(nb.0, nb.1, nb.2) != mat::WATER {
        return false;
    }
    let data = s.data(pos.0, pos.1, pos.2);
    if data == 0 {
        s.set(pos.0, pos.1, pos.2, OBSIDIAN, 0);
    } else if data <= 4 {
        s.set(pos.0, pos.1, pos.2, COBBLE, 0);
    } else {
        return false;
    }
    true
}

/// `BlockSource::updateNeighborsAt`：instatick 时只有液体（canInstatick）响应
fn update_neighbors(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) {
    let (x, y, z) = pos;
    for n in [(x - 1, y, z), (x + 1, y, z), (x, y, z + 1), (x, y, z - 1), (x, y + 1, z), (x, y - 1, z)] {
        neighbor_changed(s, n, pos, r);
    }
}

fn neighbor_changed(s: &mut Src, pos: (i32, i32, i32), nb: (i32, i32, i32), r: &mut McpeRandom) {
    let id = s.get(pos.0, pos.1, pos.2);
    if id == AIR {
        return;
    }
    let liquid = matches!(id, FLOWING_WATER | STILL_WATER | FLOWING_LAVA | STILL_LAVA);
    if s.queue.insta && !liquid {
        return;
    }
    if !liquid {
        return;
    }
    solidify(s, id, pos, nb);
    if (id == STILL_WATER || id == STILL_LAVA) && s.get(pos.0, pos.1, pos.2) == id {
        // LiquidBlockStatic::_setDynamic
        let data = s.data(pos.0, pos.1, pos.2);
        s.set(pos.0, pos.1, pos.2, id - 1, data);
        queue_add(s, pos, id - 1, tick_delay(id), r);
    }
}

fn can_spread_to(s: &Src, m: u8, x: i32, y: i32, z: i32) -> bool {
    if y < 0 || !s.has_block(x, z) {
        return false;
    }
    let t = s.mat(x, y, z);
    if t == m || t == mat::LAVA {
        return false;
    }
    !bp::water_blocking(s.get(x, y, z))
}

fn slope_distance(s: &Src, m: u8, pos: (i32, i32, i32), dist: i32, from: i32) -> i32 {
    let mut best = 1000;
    for l in 0..4 {
        if (l == 0 && from == 1) || (l == 1 && from == 0) || (l == 2 && from == 3) || (l == 3 && from == 2) {
            continue;
        }
        let (mut x, y, mut z) = pos;
        match l {
            0 => x -= 1,
            1 => x += 1,
            2 => z -= 1,
            _ => z += 1,
        }
        if bp::water_blocking(s.get(x, y, z)) {
            continue;
        }
        if s.mat(x, y, z) == m && s.data(x, y, z) == 0 {
            continue;
        }
        if bp::water_blocking(s.get(x, y - 1, z)) {
            if dist <= 3 {
                let d = slope_distance(s, m, (x, y, z), dist + 1, l);
                if best >= d {
                    best = d;
                }
            }
            continue;
        }
        return dist;
    }
    best
}

fn get_spread(s: &Src, m: u8, pos: (i32, i32, i32)) -> [bool; 4] {
    let mut cost = [1000i32; 4];
    for (i, c) in cost.iter_mut().enumerate() {
        let (mut x, y, mut z) = pos;
        match i {
            0 => x -= 1,
            1 => x += 1,
            2 => z -= 1,
            _ => z += 1,
        }
        if bp::water_blocking(s.get(x, y, z)) {
            continue;
        }
        if s.mat(x, y, z) == m && s.data(x, y, z) == 0 {
            continue;
        }
        let mut v = bp::water_blocking(s.get(x, y - 1, z)) as i32;
        if v != 0 {
            v = slope_distance(s, m, (x, y, z), 1, i as i32);
        }
        *c = v;
    }
    let mut best = if cost[1] < cost[0] { cost[1] } else { cost[0] };
    if best >= cost[2] {
        best = cost[2];
    }
    if best >= cost[3] {
        best = cost[3];
    }
    [best == cost[0], best == cost[1], best == cost[2], best == cost[3]]
}

fn try_spread_to(s: &mut Src, id: u8, x: i32, y: i32, z: i32, d: i32, r: &mut McpeRandom) {
    if !can_spread_to(s, liquid_mat(id), x, y, z) {
        return;
    }
    // 非 instatick 时：岩浆冒烟粒子（publicSource=false 直接返回）、被冲掉的方块掉落物，都不改方块
    s.set(x, y, z, id, d as u8);
    if s.queue.insta {
        update_neighbors(s, (x, y, z), r);
    }
    queue_add(s, (x, y, z), id, tick_delay(id), r);
}

/// `LiquidBlockDynamic::tick`（id 为 8 或 10）
fn dynamic_tick(s: &mut Src, pos: (i32, i32, i32), id: u8, r: &mut McpeRandom) {
    let m = liquid_mat(id);
    let (x, y, z) = pos;
    let mut d = depth(s, m, x, y, z);
    let drop_off = if m == mat::LAVA { 2 } else { 1 };
    if m == mat::LAVA {
        try_spread_fire(s, pos, r);
    }
    let mut delay = tick_delay(id);
    if d > 0 {
        let mut counter = 0;
        let mut highest = |s: &Src, x: i32, y: i32, z: i32, limit: i32| -> i32 {
            let mut dd = depth(s, m, x, y, z);
            if dd < 0 {
                return limit;
            }
            if dd == 0 {
                counter += 1;
            }
            if dd > 7 {
                dd = 0;
            }
            if dd < limit || limit < 0 {
                dd
            } else {
                limit
            }
        };
        let mut h = highest(s, x - 1, y, z, -100);
        h = highest(s, x + 1, y, z, h);
        h = highest(s, x, y, z + 1, h);
        h = highest(s, x, y, z - 1, h);
        let mut nd = h + drop_off;
        if nd > 7 || h < 0 {
            nd = -1;
        }
        let above = depth(s, m, x, y + 1, z);
        if above >= 0 {
            nd = if above <= 7 { above + 8 } else { above };
        }
        if counter > 1 && m == mat::WATER {
            let below = s.get(x, y - 1, z);
            if bp::mat_solid(below) || (bp::material(below) == m && s.data(x, y - 1, z) == 0) {
                nd = 0;
            }
        }
        if m == mat::LAVA && d < 8 && nd < 8 && nd > d && r.next_int(4) != 0 {
            delay *= 4;
        }
        if nd != d {
            d = nd;
            if d < 0 {
                s.set(x, y, z, AIR, 0);
            } else {
                s.set(x, y, z, id, d as u8);
                update_neighbors(s, pos, r);
                queue_add(s, pos, id, delay, r);
            }
        } else {
            set_static(s, pos, id);
        }
    } else {
        set_static(s, pos, id);
    }
    if can_spread_to(s, m, x, y - 1, z) {
        if m == mat::LAVA && s.mat(x, y - 1, z) == mat::WATER {
            s.set(x, y - 1, z, STONE, 0);
            return;
        }
        if d >= 8 {
            try_spread_to(s, id, x, y - 1, z, d, r);
        } else {
            try_spread_to(s, id, x, y - 1, z, d + 8, r);
        }
    } else if d >= 0 && (d == 0 || bp::water_blocking(s.get(x, y - 1, z))) {
        let spreads = get_spread(s, m, pos);
        let mut n = d + drop_off;
        if d > 7 {
            n = 1;
        }
        if n > 7 {
            return;
        }
        if spreads[0] {
            try_spread_to(s, id, x - 1, y, z, n, r);
        }
        if spreads[1] {
            try_spread_to(s, id, x + 1, y, z, n, r);
        }
        if spreads[2] {
            try_spread_to(s, id, x, y, z - 1, n, r);
        }
        if spreads[3] {
            try_spread_to(s, id, x, y, z + 1, n, r);
        }
    }
}

/// `LiquidBlockDynamic::_setStatic`
fn set_static(s: &mut Src, pos: (i32, i32, i32), id: u8) {
    let data = s.data(pos.0, pos.1, pos.2);
    s.set(pos.0, pos.1, pos.2, id + 1, data);
}
