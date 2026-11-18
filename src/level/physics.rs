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

// 液体深度语义对齐 MCPE 0.14.1 原版 LiquidBlock:
//   depth 0 = 源(静止方块)  1..7 = 水平流动(越大越远)  8..15 = 下落柱(低 3 位是层级)
//   超过 7 的水平流会枯竭  meta 上限 15
pub const MAX_FLOW: u8 = 7;
// tick 间隔 原版 LiquidBlock::getTickDelay: 水 5 / 岩浆 30(主世界)
pub const WATER_TICK_DELAY: u64 = 5;
pub const LAVA_TICK_DELAY: u64 = 30;

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

// 源 = 深度为 0 的液体(与 id 动态/静态无关) 对齐原版 getDepth==0
// 原版 WATER/STILL_WATER 只是"是否在 tick 队列"的状态 真正区分源/流的是 data
pub fn is_source(id: u8, meta: u8) -> bool {
    is_liquid(id) && depth_of(meta) == 0
}

// 深度: 低 3 位为层级 0=源 1..7=水平 8..15=下落柱
fn depth_of(meta: u8) -> u8 {
    meta & 0x0f
}

// 动态方块 id(8/10) <-> 静止方块 id(9/11) 原版 blockId+1/-1
fn static_id(id: u8) -> u8 {
    match id {
        b::WATER => b::STILL_WATER,
        b::LAVA => b::STILL_LAVA,
        other => other,
    }
}

// 液体 tick 间隔 水 5 岩浆 30
pub fn liquid_tick_delay(id: u8) -> u64 {
    if is_water(id) { WATER_TICK_DELAY } else { LAVA_TICK_DELAY }
}

// dropOff: 水 1 岩浆 2 与原版 tick() 的 dropOff
fn drop_off(id: u8) -> u8 {
    if is_water(id) { 1 } else { 2 }
}

// 重力方块 与实体模块共用同一判定(沙子/砂砾/铁砧)
pub fn is_gravity(id: u8) -> bool {
    crate::entity::falling_block::is_fallable(id)
}

// 检测一格重力方块是否失去支撑 需要转成 FallingBlock 实体
// 返回 (方块 id, meta) 由 actor 层发事件让 global 分配 eid 后回投生成实体
// 与重心的方块逐格下移不同 这里只判定"该不该开始下落" 方块清除由 actor 做
pub fn fall_check(manager: &mut ChunkManager, x: i32, y: i32, z: i32) -> Option<(u8, u8)> {
    if y <= 0 || !manager.owns(x >> 4, z >> 4) {
        return None;
    }
    let id = manager.block(x, y, z);
    if !is_gravity(id) {
        return None;
    }
    let below = manager.block(x, y - 1, z);
    if below == b::AIR || is_liquid(below) || displaced_by_liquid(below) {
        Some((id, manager.meta(x, y, z)))
    } else {
        None
    }
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
// 重力方块不再逐格下移(改由 actor 生成 FallingBlock 实体) 故这里只走液体
pub fn step(manager: &mut ChunkManager, queue: &mut UpdateQueue, x: i32, y: i32, z: i32, now: u64) -> Vec<CellChange> {
    let mut out = Vec::new();
    if !manager.owns(x >> 4, z >> 4) {
        return out;
    }
    let id = manager.block(x, y, z);
    if is_liquid(id) {
        step_liquid(manager, queue, x, y, z, now, &mut out);
    } else if is_plant(id) {
        // 原版 BushBlock::checkAlive: 基底被挖掉就枯死成空气
        let base_ok = y > 0 && can_host_plant(id, manager.block(x, y - 1, z));
        if !base_ok {
            write(manager, x, y, z, b::AIR, 0, &mut out);
        }
    }
    out
}

// 液体更新: 源与流都尝试向下/水平扩散, 流动液体失去供给则枯竭
// 原版 LiquidBlockDynamic::_getHighest 取四邻中最浅的深度
// 同种液体 meta 即深度(0=源, >7按0处理); 非同种或空返回 limit
fn get_highest(manager: &mut ChunkManager, x: i32, y: i32, z: i32, id: u8, limit: i32) -> i32 {
    if !same_liquid(manager.block(x, y, z), id) {
        return limit;
    }
    let mut depth = manager.meta(x, y, z) as i32;
    if depth > 7 {
        depth = 0; // 下落柱按源级参与高度计算
    }
    if depth < limit || limit < 0 {
        depth
    } else {
        limit
    }
}

// 原版 LiquidBlockDynamic::tick 的严格移植
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
    let depth = depth_of(meta) as i32;
    let off = drop_off(id) as i32;
    let flow_id = if is_water(id) { b::WATER } else { b::LAVA };
    let delay = liquid_tick_delay(id);

    // 1) 流动液体(depth>0): 取四邻最小深度 + dropOff 重算自身
    if depth > 0 {
        let mut highest = -100;
        highest = get_highest(manager, x - 1, y, z, id, highest);
        highest = get_highest(manager, x + 1, y, z, id, highest);
        highest = get_highest(manager, x, y, z + 1, id, highest);
        highest = get_highest(manager, x, y, z - 1, id, highest);

        let mut new_depth = highest + off;
        if new_depth > 7 || highest < 0 {
            new_depth = -1; // 断供 枯竭
        }
        // 上方有同种液体 变成下落柱
        if same_liquid(manager.block(x, y + 1, z), id) {
            let above = manager.meta(x, y + 1, z) as i32;
            new_depth = if above <= 7 { above + 8 } else { above };
        }
        // 岩浆特殊: 非源且下方是实心/源 也可能被原版的 updateCounter 逻辑归零 这里简化为水/岩浆统一
        if new_depth != depth {
            if new_depth < 0 {
                if write(manager, x, y, z, b::AIR, 0, out) {
                    // 枯竭后唤醒四周与上方 让更远的逐级回缩
                    for (dx, dz) in HORIZONTAL {
                        queue.schedule(x + dx, y, z + dz, now, delay);
                    }
                    queue.schedule(x, y + 1, z, now, delay);
                }
                return;
            } else {
                let new_meta = new_depth as u8;
                if write(manager, x, y, z, flow_id, new_meta, out) {
                    queue.schedule(x, y, z, now, delay);
                    // 深度变化唤醒四邻与上方 让沉睡的静止液体重新评估(原版 neighborChanged)
                    for (dx, dz) in HORIZONTAL {
                        queue.schedule(x + dx, y, z + dz, now, delay);
                    }
                    queue.schedule(x, y + 1, z, now, delay);
                }
            }
        } else {
            // 深度稳定 转成静止方块(原版 _setStatic) 保留 data
            write(manager, x, y, z, static_id(id), meta, out);
        }
    } else {
        // depth==0 是源 转成静止方块保留 data 保持源状态
        write(manager, x, y, z, static_id(id), meta, out);
    }

    // 2) 下方能流就优先向下 否则水平扩散
    let below = manager.block(x, y - 1, z);
    if y > 0 && displaced_by_liquid(below) {
        // 下落柱传当前深度 否则 depth+8
        let down_depth = if depth >= 8 { depth } else { depth + 8 };
        let down_meta = down_depth.min(15) as u8;
        if write(manager, x, y - 1, z, flow_id, down_meta, out) {
            queue.schedule(x, y - 1, z, now, delay);
        }
    } else if depth >= 0 && (depth == 0 || !displaced_by_liquid(below)) {
        // 源(depth=0)或下方被堵住时水平铺开: 邻居深度 = depth+dropOff 下落柱从 1 起
        let neighbor = if depth > 7 { 1 } else { (depth + off).min(7) } as u8;
        // 优先流向"下方被堵住"的方向(水会优先往有底的地方铺) 简化为按四邻 cost 排序
        let mut dirs = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)];
        dirs.sort_by_key(|&(dx, dz)| {
            let (nx, nz) = (x + dx, z + dz);
            if !displaced_by_liquid(manager.block(nx, y, nz)) {
                2 // 不能流 最后
            } else if y > 0 && !displaced_by_liquid(manager.block(nx, y - 1, nz)) {
                0 // 下方有支撑 优先铺平
            } else {
                1 // 空地 也会往下掉 次之
            }
        });
        for (dx, dz) in dirs {
            let (nx, nz) = (x + dx, z + dz);
            if !displaced_by_liquid(manager.block(nx, y, nz)) {
                continue;
            }
            // 不覆盖源或更浅的同类 只覆盖空气/可替换/更深的流
            let nb = manager.block(nx, y, nz);
            let overwrite = displaced_by_liquid(nb)
                && !(same_liquid(nb, id) && is_source(nb, manager.meta(nx, y, nz)));
            if overwrite && write(manager, nx, y, nz, flow_id, neighbor, out) {
                queue.schedule(nx, y, nz, now, delay);
            }
        }
    }
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
        // 原版: 源由 data=0 决定 与动态/静态 id 无关
        assert!(is_source(b::STILL_WATER, 0));
        assert!(is_source(b::WATER, 0));
        assert!(!is_source(b::WATER, 1));
        assert!(!is_source(b::STILL_WATER, 3));
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

    #[test]
    fn plant_dies_when_base_removed() {
        let mut m = flat_manager();
        let mut q = UpdateQueue::default();
        // y=4 草 上面 y=5 种高草 有基底不死
        m.set_block(0, 5, 0, b::TALL_GRASS);
        step(&mut m, &mut q, 0, 5, 0, 1);
        assert_eq!(m.block(0, 5, 0), b::TALL_GRASS);
        // 挖掉草基底 唤醒上方植物 再 tick 应枯死
        m.set_block(0, 4, 0, b::AIR);
        on_broken(&mut m, &mut q, 0, 4, 0, 1);
        for (x, y, z) in q.take_due(100) {
            step(&mut m, &mut q, x, y, z, 100);
        }
        assert_eq!(m.block(0, 5, 0), b::AIR, "失去基底的植物应枯死");
    }

    // ---- 重力 ----

    #[test]
    fn unsupported_sand_requests_fall_entity() {
        let mut m = flat_manager();
        // 沙子悬在 y=10 下方 y=9 是空气 应触发下落实体
        m.set_block(0, 10, 0, b::SAND);
        assert_eq!(fall_check(&mut m, 0, 10, 0), Some((b::SAND, 0)));
    }

    #[test]
    fn supported_sand_does_not_fall() {
        let mut m = flat_manager();
        // y=4 是草(solid) 放在 y=5 的沙子有支撑
        m.set_block(0, 5, 0, b::SAND);
        assert_eq!(fall_check(&mut m, 0, 5, 0), None);
        assert_eq!(m.block(0, 5, 0), b::SAND);
    }

    #[test]
    fn sand_over_liquid_requests_fall() {
        let mut m = flat_manager();
        m.set_block(0, 4, 0, b::STILL_WATER);
        m.set_block(0, 5, 0, b::SAND);
        // 液体下方不算支撑 沙子应下落
        assert_eq!(fall_check(&mut m, 0, 5, 0), Some((b::SAND, 0)));
    }

    // ---- 液体 ----

    // 5x5 围墙(内圈 x,z ∈ [-1,1]) 让水铺满后能收敛 挖源后能回缩
    fn walled_manager() -> ChunkManager {
        let mut m = flat_manager();
        for x in -2i32..=2 {
            for z in -2i32..=2 {
                if x == -2 || x == 2 || z == -2 || z == 2 {
                    m.set_block(x, 5, z, b::STONE);
                }
            }
        }
        m
    }

    // 跑物理直到队列空 每轮步进跨过一个水 tick delay
    fn settle(m: &mut ChunkManager, q: &mut UpdateQueue, limit: u64) -> u64 {
        let mut guard = 0;
        while !q.is_empty() && guard < limit {
            guard += 6;
            for (x, y, z) in q.take_due(guard) {
                step(m, q, x, y, z, guard);
            }
        }
        guard
    }

    #[test]
    fn water_source_spreads_with_depth_gradient() {
        let mut m = walled_manager();
        let mut q = UpdateQueue::default();
        // 源在中心 围墙限制在 3x3 内 铺满后必然收敛
        m.set_block(0, 5, 0, b::STILL_WATER);
        on_placed(&mut m, &mut q, 0, 5, 0, b::STILL_WATER, 0);
        let guard = settle(&mut m, &mut q, 256);
        // 源不变 四邻与角都有水
        assert_eq!(m.block(0, 5, 0), b::STILL_WATER);
        assert_eq!(m.meta(0, 5, 0), 0, "源深度 0");
        assert!(is_water(m.block(1, 5, 0)), "东邻应有水");
        assert!(is_water(m.block(0, 5, 1)), "南邻应有水");
        assert!(is_water(m.block(1, 5, 1)), "对角应有水");
        // 原版 depth 语义: 源0 < 邻1 < 角2
        assert_eq!(depth_of(m.meta(1, 5, 0)), 1, "邻格深度 1");
        assert_eq!(depth_of(m.meta(1, 5, 1)), 2, "对角深度 2");
        assert!(guard < 256, "围墙内液体应收敛 实际 {guard}");
    }

    #[test]
    fn source_removal_dries_flowing_water() {
        let mut m = walled_manager();
        let mut q = UpdateQueue::default();
        m.set_block(0, 5, 0, b::STILL_WATER);
        on_placed(&mut m, &mut q, 0, 5, 0, b::STILL_WATER, 0);
        settle(&mut m, &mut q, 256);
        assert!(is_water(m.block(1, 5, 0)));
        // 挖掉源 唤醒邻居
        m.set_block(0, 5, 0, b::AIR);
        on_broken(&mut m, &mut q, 0, 5, 0, 1000);
        settle(&mut m, &mut q, 2000);
        // 源没了 围墙内所有流动水应逐级枯竭成空气
        for x in -1..=1 {
            for z in -1..=1 {
                assert!(!is_water(m.block(x, 5, z)), "({x},{z}) 应回缩 实际 {}", m.block(x, 5, z));
            }
        }
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
