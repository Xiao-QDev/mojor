// 方块物理: 液体流动、重力方块下落、放置合法性
// 照搬 Nukkit BlockLiquid / BlockFallable / canPlaceAt 的关键语义, 但在 region 独占的单线程里执行
//
// 设计要点:
// - meta 低 4 位存液体流动层级: 0 = 源方块, 1..=MAX_FLOW 为流动液体(数字越大离源越远)
//   8..=15 为下落液体柱(用 meta & 0x07 取层级)
// - 所有读写都经 ChunkManager, 只处理属于本分片的关卡, 由 region tick 驱动定时更新
// - 更新队列用 (坐标 -> 到期 tick) 去重, 到期才处理, 处理时可能再排下一格
// - 纯逻辑不直接广播: 每步返回受影响格子, 由 actor 层包成 RegionEvent 统一同步

use std::collections::HashMap;

use super::chunk_manager::ChunkManager;
use super::generator::blocks as b;

// 流动层级上限 超过就枯竭 与 Nukkit 0.14 默认液体扩散距离一致
pub const MAX_FLOW: u8 = 5;
// 下落液体标志位 与 Nukkit 的 meta 8|level 一致
const FALLING: u8 = 0x08;

// 一格方块改动 供 actor 层广播 UpdateBlockPacket
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellChange {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub id: u8,
    pub meta: u8,
}

// 方块更新队列: 坐标 -> 到期 tick 值越大越晚处理, 重复登记取更早的到期时间
#[derive(Default)]
pub struct UpdateQueue {
    pending: HashMap<(i32, i32, i32), u64>,
}

impl UpdateQueue {
    // 登记一格在 delay tick 后更新 已排队则取更早的到期时间
    pub fn schedule(&mut self, x: i32, y: i32, z: i32, now: u64, delay: u64) {
        let due = now.saturating_add(delay);
        let entry = self.pending.entry((x, y, z)).or_insert(due);
        if due < *entry {
            *entry = due;
        }
    }

    // 取出所有到期更新 调用方保证在 region 线程内逐格处理
    pub fn take_due(&mut self, now: u64) -> Vec<(i32, i32, i32)> {
        let mut keys = Vec::new();
        self.pending.retain(|key, tick| {
            if *tick <= now {
                keys.push(*key);
                false
            } else {
                true
            }
        });
        keys
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub fn due_of(&self, x: i32, y: i32, z: i32) -> Option<u64> {
        self.pending.get(&(x, y, z)).copied()
    }
}

// ---- 判定谓词 ----

pub fn is_water(id: u8) -> bool {
    id == b::WATER || id == b::STILL_WATER
}

pub fn is_lava(id: u8) -> bool {
    id == b::LAVA || id == b::STILL_LAVA
}

pub fn is_liquid(id: u8) -> bool {
    is_water(id) || is_lava(id)
}

pub fn is_source(id: u8, meta: u8) -> bool {
    (id == b::STILL_WATER || id == b::STILL_LAVA) && meta & 0x07 == 0
}

fn flow_level(meta: u8) -> u8 {
    meta & 0x07
}

fn falling(meta: u8) -> bool {
    meta & FALLING != 0
}

// 重力方块 0.14 里只有沙子/砂砾会下落(铁砧行为另算 这里不涉及)
pub fn is_gravity(id: u8) -> bool {
    id == b::SAND || id == b::GRAVEL
}

// 这些植物必须种在合法基底上 不能叠在同类或其他植物上
pub fn is_plant(id: u8) -> bool {
    matches!(
        id,
        b::SAPLING
            | b::TALL_GRASS
            | b::DEAD_BUSH
            | b::DANDELION
            | b::RED_FLOWER
            | 39  // 棕色蘑菇
            | 40  // 红色蘑菇
            | 115 // 地狱疣
            | b::WHEAT_BLOCK
            | b::CARROT_BLOCK
            | b::POTATO_BLOCK
            | b::BEETROOT_BLOCK
            | b::PUMPKIN_STEM
            | b::MELON_STEM
            | b::SUGARCANE
            | b::DOUBLE_PLANT
    )
}

// 植物能扎根的基底 与 Nukkit BlockPlant::canPlaceAt 一致
pub fn is_plant_base(id: u8) -> bool {
    matches!(
        id,
        b::GRASS | b::DIRT | 60 /* 耕地 */ | 243 /* 灰化土 */ | 110 /* 菌丝 */ | 198 /* 草径 */
    )
}

// 甘蔗额外接受沙子 与 Nukkit BlockSugarcane 一致
fn can_host_plant(plant: u8, base: u8) -> bool {
    if plant == b::SUGARCANE {
        is_plant_base(base) || base == b::SAND
    } else {
        is_plant_base(base)
    }
}

// 放置合法性: 植物要看下方基底, 其余沿用以往的"空气/可覆盖"判定
pub fn can_place(manager: &mut ChunkManager, x: i32, y: i32, z: i32, placing_id: u8) -> bool {
    let height = super::format::chunk::CHUNK_HEIGHT as i32;
    if !(0..height).contains(&y) {
        return false;
    }
    if !manager.owns(x >> 4, z >> 4) {
        return false;
    }
    let target = manager.block(x, y, z);
    if target != b::AIR && !b::is_overridable(target) {
        return false;
    }
    if is_plant(placing_id) {
        if y == 0 {
            return false;
        }
        let below = manager.block(x, y - 1, z);
        // 关键: 植物下面必须是有基底的实心方块 不能叠在另一株植物上
        if !can_host_plant(placing_id, below) || is_plant(below) {
            return false;
        }
    }
    true
}

// 液体是否能冲走这个方块(流动时替换植物之类)
fn displaced_by_liquid(id: u8) -> bool {
    id == b::AIR || b::is_overridable(id) || matches!(id, b::TALL_GRASS | b::DEAD_BUSH | b::DANDELION | b::RED_FLOWER)
}

// ---- 写入辅助 只写属主区块 返回是否真的变了 ----

fn write(manager: &mut ChunkManager, x: i32, y: i32, z: i32, id: u8, meta: u8, out: &mut Vec<CellChange>) -> bool {
    let height = super::format::chunk::CHUNK_HEIGHT as i32;
    if !(0..height).contains(&y) || !manager.owns(x >> 4, z >> 4) {
        return false;
    }
    if manager.set_block_meta(x, y, z, id, meta) {
        out.push(CellChange { x, y, z, id, meta });
        true
    } else {
        false
    }
}

// 四邻水平偏移
const HORIZONTAL: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

// 处理一格到期更新 返回本次所有方块改动
pub fn step(manager: &mut ChunkManager, queue: &mut UpdateQueue, x: i32, y: i32, z: i32, now: u64) -> Vec<CellChange> {
    let mut out = Vec::new();
    if !manager.owns(x >> 4, z >> 4) {
        return out;
    }
    let id = manager.block(x, y, z);
    if is_gravity(id) {
        step_gravity(manager, queue, x, y, z, id, now, &mut out);
    } else if is_liquid(id) {
        step_liquid(manager, queue, x, y, z, now, &mut out);
    }
    out
}

// 重力方块: 下方是空气/液体/可替换就往下挪一格 落到实体上方就停
fn step_gravity(
    manager: &mut ChunkManager,
    queue: &mut UpdateQueue,
    x: i32,
    y: i32,
    z: i32,
    id: u8,
    now: u64,
    out: &mut Vec<CellChange>,
) {
    if y == 0 {
        return;
    }
    let below_y = y - 1;
    let below = manager.block(x, below_y, z);
    let can_fall = below == b::AIR || is_liquid(below) || displaced_by_liquid(below);
    if !can_fall {
        return;
    }
    let meta = manager.meta(x, y, z);
    // 落进液体: 液体与方块交换(液体被顶到上面那格) 否则原格变空气
    let (replacement_id, replacement_meta) = if is_liquid(below) {
        (below, manager.meta(x, below_y, z))
    } else {
        (b::AIR, 0)
    };
    write(manager, x, y, z, replacement_id, replacement_meta, out);
    write(manager, x, below_y, z, id, meta, out);
    // 下一 tick 继续检查是否还要往下落
    queue.schedule(x, below_y, z, now, 1);
    // 被顶上去的液体也要重新流动
    if is_liquid(replacement_id) {
        queue.schedule(x, y, z, now, 1);
    }
}

// 液体更新: 源与流都尝试向下/水平扩散, 流动液体失去供给则枯竭
fn step_liquid(
    manager: &mut ChunkManager,
    queue: &mut UpdateQueue,
    x: i32,
    y: i32,
    z: i32,
    now: u64,
    out: &mut Vec<CellChange>,
) {
    let id = manager.block(x, y, z);
    let meta = manager.meta(x, y, z);
    let source = is_source(id, meta);
    let level = flow_level(meta);

    // 0) 供给判定必须最先做 否则断供格先把空邻居灌满再判枯竭 会互相复活永不收敛
    // 源永远有供给; 流动液体看同层四邻或正上方是否有 源/更近(级别更低)的同类液体
    if !source && !has_feed(manager, x, y, z, id, level) {
        if write(manager, x, y, z, b::AIR, 0, out) {
            // 枯竭后唤醒周围 让更远的液体也逐级回缩
            queue.schedule(x, y, z, now, 1);
            for (dx, dz) in HORIZONTAL {
                queue.schedule(x + dx, y, z + dz, now, 1);
            }
            queue.schedule(x, y + 1, z, now, 1);
        }
        return;
    }

    // 1) 垂直下落优先 下方是空气/可替换就流下去
    if y > 0 {
        let below = manager.block(x, y - 1, z);
        if displaced_by_liquid(below) {
            let down_id = if is_water(id) { b::WATER } else { b::LAVA };
            // 源正下方给满级流动 流则递减; 下落柱用 falling 位标记
            let down_level = if source { 1 } else { level.saturating_add(1).min(MAX_FLOW) };
            if down_level <= MAX_FLOW {
                let down_meta = down_level | FALLING;
                if write(manager, x, y - 1, z, down_id, down_meta, out) {
                    queue.schedule(x, y - 1, z, now, 1);
                }
            }
        }
    }

    // 2) 水平扩散 源的邻居 1 级 流的邻居逐级 +1
    let spread_level = if falling(meta) {
        level // 下落柱每层不变级 横向照常铺开
    } else if source {
        1
    } else {
        level.saturating_add(1)
    };
    if spread_level <= MAX_FLOW {
        let flow_id = if is_water(id) { b::WATER } else { b::LAVA };
        for (dx, dz) in HORIZONTAL {
            let (nx, nz) = (x + dx, z + dz);
            let neighbor_block = manager.block(nx, y, nz);
            let neighbor_meta = manager.meta(nx, y, nz);
            // 只往空气/可替换处流 且不覆盖同源(避免同高度互相反复刷)
            if displaced_by_liquid(neighbor_block) {
                if write(manager, nx, y, nz, flow_id, spread_level, out) {
                    queue.schedule(nx, y, nz, now, 1);
                }
            } else if is_liquid(neighbor_block)
                && !is_source(neighbor_block, neighbor_meta)
                && flow_level(neighbor_meta) > spread_level
            {
                // 邻居比当前更"远" 用更短的源头刷新它 加速收敛
                if write(manager, nx, y, nz, flow_id, spread_level, out) {
                    queue.schedule(nx, y, nz, now, 1);
                }
            }
        }
    }
}

// 判断一格流动液体是否还有来源支撑
fn has_feed(manager: &mut ChunkManager, x: i32, y: i32, z: i32, id: u8, level: u8) -> bool {
    // 上方: 同种液体(源或更高级别的流)都能喂下来
    if y + 1 < super::format::chunk::CHUNK_HEIGHT as i32 {
        let up = manager.block(x, y + 1, z);
        let up_meta = manager.meta(x, y + 1, z);
        if same_liquid(up, id) && (is_source(up, up_meta) || flow_level(up_meta) < level || falling(up_meta)) {
            return true;
        }
    }
    // 同层四邻: 源或更低级别的同类液体
    for (dx, dz) in HORIZONTAL {
        let nb = manager.block(x + dx, y, z + dz);
        let nb_meta = manager.meta(x + dx, y, z + dz);
        if same_liquid(nb, id) && (is_source(nb, nb_meta) || flow_level(nb_meta) < level) {
            return true;
        }
    }
    false
}

fn same_liquid(a: u8, b: u8) -> bool {
    (is_water(a) && is_water(b)) || (is_lava(a) && is_lava(b))
}

// 放置一格液体/重力方块后 主动给它和四邻排一次更新 让物理立刻启动
pub fn on_placed(manager: &mut ChunkManager, queue: &mut UpdateQueue, x: i32, y: i32, z: i32, id: u8, now: u64) {
    if !manager.owns(x >> 4, z >> 4) {
        return;
    }
    if is_liquid(id) || is_gravity(id) {
        queue.schedule(x, y, z, now, 1);
    }
    // 任何新方块都可能改变邻居的支撑(如往沙子下挖空) 唤醒四邻与上方
    for (dx, dz) in HORIZONTAL {
        queue.schedule(x + dx, y, z + dz, now, 1);
    }
    queue.schedule(x, y + 1, z, now, 1);
}

// 挖掉一格后 唤醒四周与上方 让重力方块下落/液体回缩
pub fn on_broken(manager: &mut ChunkManager, queue: &mut UpdateQueue, x: i32, y: i32, z: i32, now: u64) {
    if !manager.owns(x >> 4, z >> 4) {
        return;
    }
    for (dx, dz) in HORIZONTAL {
        queue.schedule(x + dx, y, z + dz, now, 1);
    }
    queue.schedule(x, y + 1, z, now, 1);
    queue.schedule(x, y - 1, z, now, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::Generator;

    // flat 世界: y=0 基岩 y=1..3 泥土 y=4 草 y=5 起空气
    fn flat_manager() -> ChunkManager {
        let mut m = ChunkManager::new();
        m.set_generator(Generator::flat());
        m
    }

    // ---- UpdateQueue ----

    #[test]
    fn queue_dedups_and_takes_earliest() {
        let mut q = UpdateQueue::default();
        q.schedule(1, 2, 3, 10, 5); // due 15
        q.schedule(1, 2, 3, 11, 2); // due 13 更早
        q.schedule(4, 5, 6, 10, 1); // due 11
        assert_eq!(q.due_of(1, 2, 3), Some(13));
        assert_eq!(q.take_due(12), vec![(4, 5, 6)]);
        assert_eq!(q.take_due(13), vec![(1, 2, 3)]);
        assert!(q.is_empty());
    }

    // ---- 判定 ----

    #[test]
    fn predicates_classify_blocks() {
        assert!(is_liquid(b::WATER) && is_liquid(b::STILL_LAVA));
        assert!(is_source(b::STILL_WATER, 0));
        assert!(!is_source(b::WATER, 0));
        assert!(is_gravity(b::SAND) && is_gravity(b::GRAVEL));
        assert!(!is_gravity(b::STONE));
        assert!(is_plant(b::TALL_GRASS) && is_plant(b::WHEAT_BLOCK));
        assert!(is_plant_base(b::GRASS) && is_plant_base(60));
        assert!(!is_plant_base(b::TALL_GRASS));
    }

    // ---- 放置合法性 ----

    #[test]
    fn plant_rejects_stacking_on_plant_but_allows_soil() {
        let mut m = flat_manager();
        // flat 地形 y=4 是草 可在上面(y=5)种高草
        assert_eq!(m.block(0, 4, 0), b::GRASS);
        assert!(can_place(&mut m, 0, 5, 0, b::TALL_GRASS));
        // 基底换成高草 再叠一层被拒(用户要修的"草上放草")
        m.set_block(0, 5, 0, b::TALL_GRASS);
        assert!(!can_place(&mut m, 0, 6, 0, b::TALL_GRASS));
        // 石头上也种不了高草
        m.set_block(3, 4, 0, b::STONE);
        assert!(!can_place(&mut m, 3, 5, 0, b::DANDELION));
        // 普通方块不受植物规则限制 只要目标格是空气
        assert!(can_place(&mut m, 5, 5, 5, b::STONE));
        // 目标格被占则不能放
        m.set_block(6, 5, 5, b::STONE);
        assert!(!can_place(&mut m, 6, 5, 5, b::STONE));
    }

    // ---- 重力 ----

    #[test]
    fn sand_falls_one_cell_per_tick_until_supported() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        // 沙子悬在 y=10 下方 y=4 是草 应落到 y=5
        m.set_block(0, 10, 0, b::SAND);
        on_placed(&mut m, &mut q, 0, 10, 0, b::SAND, 0);
        let mut guard = 0;
        while !q.is_empty() && guard < 64 {
            guard += 1;
            let due = q.take_due(guard);
            for (x, y, z) in due {
                step(&mut m, &mut q, x, y, z, guard);
            }
        }
        assert_eq!(m.block(0, 5, 0), b::SAND, "沙子应停在草上方 y=5");
        assert_eq!(m.block(0, 10, 0), b::AIR);
        assert!(guard < 64, "应当收敛 实际 {guard} 轮");
    }

    #[test]
    fn supported_sand_does_not_move() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        // y=4 是草(solid) 放在 y=5 的沙子有支撑
        m.set_block(0, 5, 0, b::SAND);
        let changes = step(&mut m, &mut q, 0, 5, 0, 0);
        assert!(changes.is_empty());
        assert_eq!(m.block(0, 5, 0), b::SAND);
    }

    // ---- 液体 ----

    #[test]
    fn water_source_spreads_and_falls() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        // 地板 y=4 是草 源在 y=5
        m.set_block(0, 5, 0, b::STILL_WATER);
        on_placed(&mut m, &mut q, 0, 5, 0, b::STILL_WATER, 0);
        let mut guard = 0;
        while !q.is_empty() && guard < 128 {
            guard += 1;
            for (x, y, z) in q.take_due(guard) {
                step(&mut m, &mut q, x, y, z, guard);
            }
        }
        // 源本身不变
        assert_eq!(m.block(0, 5, 0), b::STILL_WATER);
        // 同层四邻有流动水
        assert!(is_water(m.block(1, 5, 0)), "东侧应有水");
        assert!(is_water(m.block(0, 5, 1)), "南侧应有水");
        assert!(guard < 128, "液体应收敛 实际 {guard} 轮");
    }

    #[test]
    fn source_removal_dries_flowing_water() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        m.set_block(0, 5, 0, b::STILL_WATER);
        on_placed(&mut m, &mut q, 0, 5, 0, b::STILL_WATER, 0);
        // 先让它铺开
        let mut guard = 0;
        while !q.is_empty() && guard < 128 {
            guard += 1;
            for (x, y, z) in q.take_due(guard) {
                step(&mut m, &mut q, x, y, z, guard);
            }
        }
        assert!(is_water(m.block(1, 5, 0)));
        // 挖掉源 唤醒邻居
        m.set_block(0, 5, 0, b::AIR);
        on_broken(&mut m, &mut q, 0, 5, 0, guard);
        guard = 0;
        while !q.is_empty() && guard < 256 {
            guard += 1;
            for (x, y, z) in q.take_due(guard) {
                step(&mut m, &mut q, x, y, z, guard);
            }
        }
        // 源没了 流动水应枯竭
        assert_eq!(m.block(1, 5, 0), b::AIR, "流动水应回缩");
    }

    #[test]
    fn liquid_does_not_enter_solid() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        m.set_block(0, 5, 0, b::STILL_WATER);
        // 东侧是实心墙
        m.set_block(1, 5, 0, b::STONE);
        let _out = step(&mut m, &mut q, 0, 5, 0, 0);
        assert_eq!(m.block(1, 5, 0), b::STONE, "水不能替换石头");
    }
}
