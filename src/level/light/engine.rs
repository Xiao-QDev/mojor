// 光照引擎 移植 vanilla 26.3 的 LightEngine / SkyLightEngine / BlockLightEngine
// 双队列增量 BFS：decrease 队列清旧光、increase 队列填新光，先减后增两段跑完
// 每个队列项是 (node, data)：node 是快照内坐标，data 按 vanilla QueueEntry 位打包
//   bits 0..3 level | bits 4..9 六个方向（DOWN,UP,NORTH,SOUTH,WEST,EAST）| bit 11 emission 标记
// 0.14 的适配差异（全高存储 0..128）：
//   无 section 存储 → 不移植 propagateFromEmptySections / setLightEnabled 的 section 填充
//   无方块形状系统 → 传播遮挡只由 opacity 表达（不透明方块 opacity=16 光进不去）
use std::collections::VecDeque;

use super::opacity::{emission, opacity};
use super::service::BlockChange;
use super::snapshot::{Channel, Snapshot, SNAP_W};
use super::sources;

const MAX_LEVEL: u8 = 15;

// 方向位 与 vanilla Direction.values() 的 ordinal 顺序一致
const DIR_DOWN: u64 = 1 << 4;
const DIR_UP: u64 = 1 << 5;
const DIR_NORTH: u64 = 1 << 6;
const DIR_SOUTH: u64 = 1 << 7;
const DIR_WEST: u64 = 1 << 8;
const DIR_EAST: u64 = 1 << 9;
const DIRECTIONS: [u64; 6] = [DIR_DOWN, DIR_UP, DIR_NORTH, DIR_SOUTH, DIR_WEST, DIR_EAST];
const ALL_DIRECTIONS: u64 = DIR_DOWN | DIR_UP | DIR_NORTH | DIR_SOUTH | DIR_WEST | DIR_EAST;
const FLAG_FROM_EMISSION: u64 = 1 << 11;

// 快照坐标打包 node：x 6bit << 13 | z 6bit << 7 | y 7bit
#[inline]
fn node(x: usize, y: usize, z: usize) -> u32 {
    ((x as u32) << 13) | ((z as u32) << 7) | y as u32
}
#[inline]
fn node_x(n: u32) -> usize {
    (n >> 13) as usize
}
#[inline]
fn node_y(n: u32) -> usize {
    (n & 0x7f) as usize
}
#[inline]
fn node_z(n: u32) -> usize {
    ((n >> 7) & 0x3f) as usize
}

// QueueEntry 构造
#[inline]
fn with_level(data: u64, level: u8) -> u64 {
    (data & !0xf) | (level as u64 & 0xf)
}
fn decrease_all(old: u8) -> u64 {
    with_level(ALL_DIRECTIONS, old)
}
// vanilla decreaseSkipOneDirection：跳过一个方向
fn decrease_skip(old: u8, skip: u64) -> u64 {
    with_level(ALL_DIRECTIONS & !skip, old)
}
fn increase_skip(new: u8, skip: u64) -> u64 {
    with_level(ALL_DIRECTIONS & !skip, new)
}
// vanilla increaseOnlyOneDirection：只朝一个方向
fn increase_only(new: u8, dir: u64) -> u64 {
    with_level(dir, new)
}
fn increase_from_emission(new: u8) -> u64 {
    with_level(ALL_DIRECTIONS | FLAG_FROM_EMISSION, new)
}

#[inline]
fn opposite(dir: u64) -> u64 {
    match dir {
        DIR_DOWN => DIR_UP,
        DIR_UP => DIR_DOWN,
        DIR_NORTH => DIR_SOUTH,
        DIR_SOUTH => DIR_NORTH,
        DIR_WEST => DIR_EAST,
        DIR_EAST => DIR_WEST,
        _ => 0,
    }
}

// 邻居坐标 出了快照或世界顶底返回 None（该方向不传播）
fn neighbor(from: u32, dir: u64) -> Option<u32> {
    let (x, y, z) = (node_x(from), node_y(from), node_z(from));
    let (nx, ny, nz) = match dir {
        DIR_DOWN => (x, y.checked_sub(1)?, z),
        DIR_UP => (x, y + 1, z),
        DIR_NORTH => (x, y, z.checked_sub(1)?),
        DIR_SOUTH => (x, y, z + 1),
        DIR_WEST => (x.checked_sub(1)?, y, z),
        DIR_EAST => (x + 1, y, z),
        _ => return None,
    };
    if ny >= 128 || nx >= SNAP_W || nz >= SNAP_W {
        None
    } else {
        Some(node(nx, ny, nz))
    }
}

pub struct Engine {
    snap: Snapshot,
    // 队列按通道分开 两通道互不污染 对齐 vanilla 的双引擎实例
    decrease: [VecDeque<(u32, u64)>; 2],
    increase: [VecDeque<(u32, u64)>; 2],
}

impl Engine {
    // 新区块的全量光照：源列直填 + 自发光 + 边界拉光 + BFS 收敛
    pub fn init(snap: Snapshot) -> Snapshot {
        let mut engine = Engine {
            snap,
            decrease: [VecDeque::new(), VecDeque::new()],
            increase: [VecDeque::new(), VecDeque::new()],
        };
        engine.init_sky();
        engine.init_block();
        // 先 block 后 sky 与 vanilla 两个引擎的独立运行等价
        engine.run(Channel::Block);
        engine.run(Channel::Sky);
        engine.snap
    }

    // 玩家改方块的增量修正
    pub fn update_blocks(snap: Snapshot, changes: &[BlockChange]) -> Snapshot {
        let mut engine = Engine {
            snap,
            decrease: [VecDeque::new(), VecDeque::new()],
            increase: [VecDeque::new(), VecDeque::new()],
        };
        engine.apply_changes(changes);
        engine.run(Channel::Block);
        engine.run(Channel::Sky);
        engine.snap
    }

    // ---- Init：新区块 ----

    // 源列直填 15 + 边界源入队 对齐 SkyLightEngine.propagateLightSources
    fn init_sky(&mut self) {
        for lx in 0..16usize {
            for lz in 0..16usize {
                let (sx, sz) = (16 + lx, 16 + lz);
                let lowest = self.snap.column_source(sx, sz);
                let neighbor_lowest = self.snap.neighbor_lowest(sx, sz);
                // y >= lowest 的格子全是天空源 lowest=-1（全透列）时整列直填
                for y in lowest.max(0) as usize..128usize {
                    self.snap
                        .set_level(Channel::Sky, sx, y, sz, MAX_LEVEL);
                    // 源列底一格 或 低于邻居源列的格子才需要向外传播
                    if y as i16 == lowest || (y as i16) < neighbor_lowest {
                        let n = node(sx, y, sz);
                        // ADD_SKY_SOURCE_ENTRY：向上不用传（上面也是源）
                        self.enqueue_increase(Channel::Sky, n, increase_skip(MAX_LEVEL, DIR_UP));
                    }
                }
            }
        }
        self.pull_in_border(Channel::Sky);
    }

    // 中心区块的发光方块 对齐 BlockLightEngine.propagateLightSources 的 findBlockLightSources
    fn init_block(&mut self) {
        for lx in 0..16usize {
            for lz in 0..16usize {
                let (sx, sz) = (16 + lx, 16 + lz);
                for y in 0..128usize {
                    let em = emission(self.snap.block(sx, y, sz));
                    if em > 0 {
                        self.enqueue_increase(Channel::Block, node(sx, y, sz), increase_from_emission(em));
                    }
                }
            }
        }
        self.pull_in_border(Channel::Block);
    }

    // 中心区块四侧边界每格入一次"拉光" decrease(1) 全方向
    // PULL_LIGHT_IN_ENTRY 的语义：邻居更亮就转 increase 把光拉进来
    // 让邻块已有的光照流进刚启用的新区块
    fn pull_in_border(&mut self, channel: Channel) {
        for y in 0..128usize {
            for t in 0..16usize {
                for &(sx, sz) in
                    &[(16 + t, 16), (16 + t, 31), (16, 16 + t), (31, 16 + t)]
                {
                    let n = node(sx, y, sz);
                    self.enqueue_decrease(channel, n, decrease_all(1));
                }
            }
        }
    }

    // ---- Update：改方块 ----

    fn apply_changes(&mut self, changes: &[BlockChange]) {
        let mut checked: Vec<u32> = Vec::new();
        let mut columns: Vec<(usize, usize)> = Vec::new();
        for change in changes {
            // y 越界在 ChunkManager 侧已拦 这里只兜坐标换算
            let Some((sx, sz)) = self.snap.world_to_snap(change.x, change.z) else {
                continue;
            };
            let sy = change.y as usize;
            self.snap.set_block(sx, sy, sz, change.new_id);
            checked.push(node(sx, sy, sz));
            if !columns.contains(&(sx, sz)) {
                columns.push((sx, sz));
            }
        }
        // 改动列重算源列 变了就整列修正（removeSourcesBelow / addSourcesAbove）
        for &(sx, sz) in &columns {
            let new_lowest =
                sources::column_lowest_source(|y| self.snap.block(sx, y as usize, sz));
            let old = self.snap.column_source(sx, sz);
            if new_lowest != old {
                self.snap.set_column_source(sx, sz, new_lowest);
                self.update_sources_in_column(sx, sz, new_lowest);
            }
        }
        for n in checked {
            self.check_node_block(n);
            self.check_node_sky(n);
        }
    }

    // 对齐 ChunkSkyLightSources.update + SkyLightEngine.checkNode 的列修正
    fn update_sources_in_column(&mut self, sx: usize, sz: usize, lowest: i16) {
        self.remove_sources_below(sx, sz, lowest);
        self.add_sources_above(sx, sz, lowest);
    }

    // 源列抬高：旧的 15 全清掉并级联减光
    fn remove_sources_below(&mut self, sx: usize, sz: usize, lowest: i16) {
        if lowest > 0 {
            for y in (0..lowest as usize).rev() {
                // 碰到不是 15 的格子说明旧源到此为止
                if self.snap.level(Channel::Sky, sx, y, sz) != MAX_LEVEL {
                    return;
                }
                self.snap.set_level(Channel::Sky, sx, y, sz, 0);
                let n = node(sx, y, sz);
                // 源列最底那格向下减（REMOVE_TOP_SKY_SOURCE_ENTRY）其余跳过向上
                let entry = if y as i16 == lowest - 1 {
                    decrease_all(MAX_LEVEL)
                } else {
                    decrease_skip(MAX_LEVEL, DIR_UP)
                };
                self.enqueue_decrease(Channel::Sky, n, entry);
            }
        }
    }

    // 源列降低：y >= lowest 的格子重新直填 15 边界格子入队
    fn add_sources_above(&mut self, sx: usize, sz: usize, lowest: i16) {
        let neighbor_lowest = self.snap.neighbor_lowest(sx, sz);
        for y in lowest.max(0) as usize..128usize {
            // 碰到已是 15 的格子说明上面已处理过
            if self.snap.level(Channel::Sky, sx, y, sz) == MAX_LEVEL {
                return;
            }
            self.snap
                .set_level(Channel::Sky, sx, y, sz, MAX_LEVEL);
            if (y as i16) < neighbor_lowest || y as i16 == lowest {
                let n = node(sx, y, sz);
                self.enqueue_increase(Channel::Sky, n, increase_skip(MAX_LEVEL, DIR_UP));
            }
        }
    }

    // recheck 一格的方块光 对齐 BlockLightEngine.checkNode
    fn check_node_block(&mut self, n: u32) {
        let (sx, y, sz) = (node_x(n), node_y(n), node_z(n));
        let em = emission(self.snap.block(sx, y, sz));
        let old = self.snap.level(Channel::Block, sx, y, sz);
        if em < old {
            // 旧光比新 emission 大 清掉重新传播
            self.snap.set_level(Channel::Block, sx, y, sz, 0);
            self.enqueue_decrease(Channel::Block, n, decrease_all(old));
        } else {
            // emission 抬高或不变 opacity 可能变了 拉光重算
            self.enqueue_decrease(Channel::Block, n, decrease_all(1));
        }
        if em > 0 {
            self.enqueue_increase(Channel::Block, n, increase_from_emission(em));
        }
    }

    // recheck 一格的天空光 对齐 SkyLightEngine.checkNode
    // 前提：改动列的源列修正已经先行完成
    fn check_node_sky(&mut self, n: u32) {
        let (sx, y, sz) = (node_x(n), node_y(n), node_z(n));
        let lowest = self.snap.column_source(sx, sz);
        if y as i16 >= lowest {
            // 仍是源（或成为源）：清旧扩散 再重新向外填 15
            // REMOVE_SKY_SOURCE_ENTRY / ADD_SKY_SOURCE_ENTRY 都跳过向上
            self.enqueue_decrease(Channel::Sky, n, decrease_skip(MAX_LEVEL, DIR_UP));
            self.enqueue_increase(Channel::Sky, n, increase_skip(MAX_LEVEL, DIR_UP));
        } else {
            let old = self.snap.level(Channel::Sky, sx, y, sz);
            if old > 0 {
                self.snap.set_level(Channel::Sky, sx, y, sz, 0);
                self.enqueue_decrease(Channel::Sky, n, decrease_all(old));
            } else {
                self.enqueue_decrease(Channel::Sky, n, decrease_all(1));
            }
        }
    }

    // ---- 队列与传播 ----

    #[inline]
    fn enqueue_decrease(&mut self, channel: Channel, n: u32, data: u64) {
        self.decrease[channel as usize].push_back((n, data));
    }

    #[inline]
    fn enqueue_increase(&mut self, channel: Channel, n: u32, data: u64) {
        self.increase[channel as usize].push_back((n, data));
    }

    // vanilla runLightUpdates 的两段循环 先把减队列清空再把增队列清空
    fn run(&mut self, channel: Channel) {
        let which = channel as usize;
        while let Some((n, data)) = self.decrease[which].pop_front() {
            self.propagate_decrease(channel, n, data);
        }
        while let Some((n, data)) = self.increase[which].pop_front() {
            let (x, y, z) = (node_x(n), node_y(n), node_z(n));
            let mut from_level = self.snap.level(channel, x, y, z);
            let target = (data & 0xf) as u8;
            // emission 来源的 increase 自动补上自身亮度
            if data & FLAG_FROM_EMISSION != 0 && from_level < target {
                self.snap.set_level(channel, x, y, z, target);
                from_level = target;
            }
            if from_level == target {
                self.propagate_increase(channel, n, data, from_level);
            }
        }
    }

    // vanilla propagateIncrease：向指定方向扩散 new = from - opacity(to)
    fn propagate_increase(&mut self, channel: Channel, from: u32, data: u64, from_level: u8) {
        for dir in DIRECTIONS {
            if data & dir == 0 {
                continue;
            }
            let Some(to) = neighbor(from, dir) else {
                continue;
            };
            let (tx, ty, tz) = (node_x(to), node_y(to), node_z(to));
            let to_level = self.snap.level(channel, tx, ty, tz);
            // 乐观筛：opacity 至少是 1 先排除绝对不可能的（vanilla 用 int 运算 这里的减法可能为负）
            if from_level as i32 - 1 > to_level as i32 {
                let new_to = from_level as i32 - opacity(self.snap.block(tx, ty, tz)) as i32;
                if new_to > to_level as i32 {
                    self.snap.set_level(channel, tx, ty, tz, new_to as u8);
                    if new_to > 1 {
                        self.enqueue_increase(channel, to, increase_skip(new_to as u8, opposite(dir)));
                    }
                }
            }
        }
    }

    // vanilla propagateDecrease：清旧光 遇到更亮的邻居转 increase 让它重传回来
    fn propagate_decrease(&mut self, channel: Channel, from: u32, data: u64) {
        let old_from = (data & 0xf) as u8;
        for dir in DIRECTIONS {
            if data & dir == 0 {
                continue;
            }
            let Some(to) = neighbor(from, dir) else {
                continue;
            };
            let (tx, ty, tz) = (node_x(to), node_y(to), node_z(to));
            let to_level = self.snap.level(channel, tx, ty, tz);
            if to_level == 0 {
                continue;
            }
            if to_level as i32 <= old_from as i32 - 1 {
                self.snap.set_level(channel, tx, ty, tz, 0);
                if channel == Channel::Block {
                    // 方块光有 emission 保护：发光体被清时立刻重新入队
                    let em = emission(self.snap.block(tx, ty, tz));
                    if em < to_level {
                        self.enqueue_decrease(channel, to, decrease_skip(to_level, opposite(dir)));
                    }
                    if em > 0 {
                        self.enqueue_increase(channel, to, increase_from_emission(em));
                    }
                } else {
                    self.enqueue_decrease(channel, to, decrease_skip(to_level, opposite(dir)));
                }
            } else {
                // 邻居比我亮 增长它 让它只朝我这边重传
                self.enqueue_increase(channel, to, increase_only(to_level, opposite(dir)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::chunk_manager::ChunkManager;
    use crate::level::format::chunk::Chunk;

    const SX: usize = 16; // 中心区块在快照里的起点
    const SZ: usize = 16;

    // 造一个 3x3 测试环境（无存档 manager 邻居手工塞）
    // lit_neighbors 为 true 时邻居天空光预填 15 模拟真实流程里
    // 邻居区块已经 Init 完 grab 拿到的现成光照
    fn world_with(lit_neighbors: bool) -> Snapshot {
        let mut manager = ChunkManager::new();
        for dx in -1..=1 {
            for dz in -1..=1 {
                if lit_neighbors && !(dx == 0 && dz == 0) {
                    let mut chunk = Chunk::empty(dx, dz);
                    // 天空光 nibble 全填 0xff 即每格 15
                    chunk.parts_mut().2.fill(0xff);
                    manager.insert_loaded_chunk(dx, dz, Some(chunk));
                } else {
                    manager.insert_loaded_chunk(dx, dz, Some(Chunk::empty(dx, dz)));
                }
            }
        }
        Snapshot::grab(&manager, 0, 0)
    }

    fn air_world() -> Snapshot {
        world_with(false)
    }

    #[test]
    fn init_fills_exposed_air_and_leaves_surface_dark() {
        let mut snap = world_with(true);
        // 中心列铺一层石头 当地表
        for lx in 0..16usize {
            for lz in 0..16usize {
                snap.set_block(16 + lx, 4, 16 + lz, 1);
            }
        }
        // 重算源列（真实流程由 grab 完成 这里直接改）
        for lx in 0..16usize {
            for lz in 0..16usize {
                let (sx, sz) = (16 + lx, 16 + lz);
                let lowest = sources::column_lowest_source(|y| snap.block(sx, y as usize, sz));
                snap.set_column_source(sx, sz, lowest);
            }
        }
        let snap = Engine::init(snap);

        // vanilla 语义：地表方块自身 0 它上面的空气 15
        assert_eq!(snap.level(Channel::Sky, SX, 5, SZ), 15);
        assert_eq!(snap.level(Channel::Sky, SX, 4, SZ), 0);
        // 石板下的空气从邻居列的 15 横向漏光 只差一步衰减
        // 邻居全透列自己就是源 边界拉光把它拽进来
        assert_eq!(snap.level(Channel::Sky, SX, 3, SZ), 14);
    }

    #[test]
    fn sky_attenuates_under_water() {
        let mut snap = air_world();
        // 整个快照铺水 否则旁边露天空气列会横向漏光进来 看不出衰减
        for x in 0..SNAP_W {
            for z in 0..SNAP_W {
                for y in 40..=42 {
                    snap.set_block(x, y, z, 8);
                }
            }
        }
        for x in 0..SNAP_W {
            for z in 0..SNAP_W {
                let lowest = sources::column_lowest_source(|y| snap.block(x, y as usize, z));
                snap.set_column_source(x, z, lowest);
            }
        }
        let snap = Engine::init(snap);

        assert_eq!(snap.level(Channel::Sky, SX, 43, SZ), 15);
        // 每格水衰减 1 水下空气继续衰减
        assert_eq!(snap.level(Channel::Sky, SX, 42, SZ), 14);
        assert_eq!(snap.level(Channel::Sky, SX, 41, SZ), 13);
        assert_eq!(snap.level(Channel::Sky, SX, 40, SZ), 12);
        assert_eq!(snap.level(Channel::Sky, SX, 39, SZ), 11);
    }

    #[test]
    fn init_pulls_neighbor_light_into_border() {
        // 邻居区块预置火把光照 中心区块 init 时应通过边界拉光收到
        let mut manager = ChunkManager::new();
        let mut center = Chunk::empty(0, 0);
        center.set_block(0, 10, 0, 50); // 中心区块角落的火把
        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dz == 0 {
                    manager.insert_loaded_chunk(0, 0, Some(center.clone()));
                } else {
                    manager.insert_loaded_chunk(dx, dz, Some(Chunk::empty(dx, dz)));
                }
            }
        }
        let snap = Snapshot::grab(&manager, 0, 0);
        let snap = Engine::init(snap);
        // 火把自己的格子和相邻格都有方块光
        assert_eq!(snap.level(Channel::Block, SX, 10, SZ), 14);
        assert_eq!(snap.level(Channel::Block, SX + 1, 10, SZ), 13);
    }

    #[test]
    fn update_places_and_removes_a_torch() {
        let snap = air_world();
        let snap = Engine::init(snap);
        assert_eq!(snap.level(Channel::Block, SX, 10, SZ), 0);

        // 放火把
        let place = BlockChange { x: 0, y: 10, z: 0, old_id: 0, new_id: 50 };
        let snap = Engine::update_blocks(snap, &[place]);
        assert_eq!(snap.level(Channel::Block, SX, 10, SZ), 14);
        assert_eq!(snap.level(Channel::Block, SX + 1, 10, SZ), 13);
        assert_eq!(snap.level(Channel::Block, SX + 14, 10, SZ), 0);

        // 拆火把 周围全部回到 0
        let remove = BlockChange { x: 0, y: 10, z: 0, old_id: 50, new_id: 0 };
        let snap = Engine::update_blocks(snap, &[remove]);
        assert_eq!(snap.level(Channel::Block, SX, 10, SZ), 0);
        assert_eq!(snap.level(Channel::Block, SX + 1, 10, SZ), 0);
    }

    #[test]
    fn update_occluding_block_cuts_the_sky_column() {
        let snap = air_world();
        let snap = Engine::init(snap);
        assert_eq!(snap.level(Channel::Sky, SX, 50, SZ), 15);

        // y50 放一块石头：源列被截断 石头自身不透光
        let place = BlockChange { x: 0, y: 50, z: 0, old_id: 0, new_id: 1 };
        let snap = Engine::update_blocks(snap, &[place]);
        assert_eq!(snap.level(Channel::Sky, SX, 50, SZ), 0);
        // 石头正下方一格 从邻列的 15 横向漏光 只差一步衰减
        assert_eq!(snap.level(Channel::Sky, SX, 49, SZ), 14);
        assert_eq!(snap.level(Channel::Sky, SX, 51, SZ), 15);
        // 邻列自己是天空源 不被级联清光
        assert_eq!(snap.level(Channel::Sky, SX + 1, 49, SZ), 15);

        // 拆掉后恢复
        let remove = BlockChange { x: 0, y: 50, z: 0, old_id: 1, new_id: 0 };
        let snap = Engine::update_blocks(snap, &[remove]);
        assert_eq!(snap.level(Channel::Sky, SX, 50, SZ), 15);
        assert_eq!(snap.level(Channel::Sky, SX, 49, SZ), 15);
        assert_eq!(snap.level(Channel::Sky, SX + 1, 49, SZ), 15);
    }

    #[test]
    fn init_spreads_sky_light_into_a_shaft_horizontally() {
        let mut snap = air_world();
        // 除中心列外整块铺到 y30 中心列留一口深井到 y0 井口 y32 加盖
        // 加盖后井列不再是天空源 光只能从井口边缘横着渗进来
        for lx in 0..16usize {
            for lz in 0..16usize {
                for y in 0..31usize {
                    // 井体 (lx,lz)==(0,0) 保持空气
                    if lx == 0 && lz == 0 {
                        continue;
                    }
                    snap.set_block(16 + lx, y, 16 + lz, 1);
                }
            }
        }
        snap.set_block(SX, 32, SZ, 1);
        for lx in 0..16usize {
            for lz in 0..16usize {
                let (sx, sz) = (16 + lx, 16 + lz);
                let lowest = sources::column_lowest_source(|y| snap.block(sx, y as usize, sz));
                snap.set_column_source(sx, sz, lowest);
            }
        }
        let snap = Engine::init(snap);

        // 盖子上方直填 15 盖子自身不透光
        assert_eq!(snap.level(Channel::Sky, SX, 33, SZ), 15);
        assert_eq!(snap.level(Channel::Sky, SX, 32, SZ), 0);
        // 井壁顶 y31 是源列 15 横传进井 14 然后逐格往下衰减
        assert_eq!(snap.level(Channel::Sky, SX, 31, SZ), 14);
        assert_eq!(snap.level(Channel::Sky, SX, 30, SZ), 13);
        assert_eq!(snap.level(Channel::Sky, SX, 21, SZ), 4);
        assert_eq!(snap.level(Channel::Sky, SX, 20, SZ), 3);
        assert_eq!(snap.level(Channel::Sky, SX, 0, SZ), 0);
    }

    // dampening 表由 sources 模块用于源列隔断 编译期确认可见性
    #[test]
    fn dampening_table_is_reachable() {
        assert_eq!(super::super::opacity::dampening(0), 0);
        assert_eq!(super::super::opacity::opacity(0), 1);
    }
}
