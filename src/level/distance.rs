// 区块加载距离场 照搬 vanilla 26.3 的 DistanceManager.PlayerTicketTracker
// 对应关系：FixedPlayerDistanceChunkTracker -> levels 字段与 setLevel
//          DynamicGraphMinFixedPoint -> 传播引擎 checkEdge/runUpdates
//          ChunkTracker.checkNeighborsAfterUpdate -> 八邻域 +1 传播
//
// 玩家所在区块是源 level 0 邻居 +1 传播 增量更新 不重算整个世界
// level <= view_distance 的区块挂加载票(loading) 就是手电筒照到的地方
// 与下发圈(欧氏圆)分开 边缘区块只加载不下发 省带宽也不闪
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;

// 虚拟源节点 vanilla 用 ChunkPos.INVALID_CHUNK_POS 当所有玩家的总入口
pub const SOURCE: i64 = i64::MAX;
// 没有算出传播结果时的占位值 vanilla 是 (byte)-1 即 255
const NO_COMPUTED_LEVEL: u8 = 255;

// 区块坐标打包成 i64 vanilla ChunkPos.pack
pub fn pack_chunk(x: i32, z: i32) -> i64 {
    ((x as i64) << 32) | (z as u32 as i64)
}

// vanilla ChunkPos.unpack
pub fn unpack_chunk(pos: i64) -> (i32, i32) {
    ((pos >> 32) as i32, pos as u32 as i32)
}

// 每档优先级一个队列 出队总取最小档
// 照搬 vanilla 的 LeveledPriorityQueue 每档是 LongLinkedOpenHashSet
// Set 语义：移除不在的节点静默无操作 重复入队不叠加
// slots 索引与队列保持一致 所有移除以索引为准
struct LeveledPriorityQueue {
    // 档位 -> 该档的节点 插入有序
    queues: Vec<VecDeque<i64>>,
    // 节点 -> 所在档位 不在队列里的节点没有条目
    slots: HashMap<i64, usize>,
    len: usize,
}

impl LeveledPriorityQueue {
    fn new(level_count: usize) -> Self {
        Self {
            queues: (0..level_count).map(|_| VecDeque::new()).collect(),
            slots: HashMap::new(),
            len: 0,
        }
    }

    fn enqueue(&mut self, node: i64, priority: usize) {
        // 已在同一档 Set.add 是 no-op
        if self.slots.get(&node) == Some(&priority) {
            return;
        }
        // 换档先按索引摘掉旧的 再入新档
        self.dequeue(node, priority);
    }

    // vanilla 的 dequeue(node, key, upperBound) key 是旧档位
    // 调用方给的旧档位可能与索引不符 以索引为准 静默容错
    fn dequeue(&mut self, node: i64, new: usize) {
        if let Some(&old) = self.slots.get(&node) {
            if old == new {
                return;
            }
            if let Some(queue) = self.queues.get_mut(old) {
                queue.retain(|&n| n != node);
            }
            self.len -= 1;
            self.slots.remove(&node);
        }
        self.queues[new].push_back(node);
        self.slots.insert(node, new);
        self.len += 1;
    }

    fn remove_first(&mut self) -> Option<i64> {
        let level = self.queues.iter().position(|queue| !queue.is_empty())?;
        let node = self.queues[level].pop_front()?;
        self.slots.remove(&node);
        self.len -= 1;
        Some(node)
    }

    // vanilla 的 removeFirstLong 队列非空由调用方保证
    fn remove_first_long(&mut self) -> i64 {
        self.remove_first().unwrap_or(i64::MIN)
    }

    // 只移除不重排 vanilla else 分支的 dequeue(node, key, levelCount)
    fn remove_only(&mut self, node: i64) {
        if let Some(old) = self.slots.remove(&node) {
            if let Some(queue) = self.queues.get_mut(old) {
                queue.retain(|&n| n != node);
            }
            self.len -= 1;
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// 玩家区块距离场 源是玩家所在区块
// players_per_chunk 记录每个区块里有哪些玩家 没有玩家的区块不占内存
pub struct PlayerChunkTracker {
    // 每个区块里的玩家 DistanceManager.playersPerChunk
    players_per_chunk: HashMap<i64, HashSet<SocketAddr>>,
    // 距离场算出的 level 默认 max_distance + 2 视为不可达
    levels: HashMap<i64, u8>,
    // 已算出还没落地的 level NO_COMPUTED_LEVEL 表示没有
    computed_levels: HashMap<i64, u8>,
    priority_queue: LeveledPriorityQueue,
    // vanilla PlayerTicketTracker(32) 的固定上限
    max_distance: i32,
    // 当前视距 决定哪些 level 挂加载票
    view_distance: i32,
    // 加载票集合 照搬 haveTicketFor(level <= viewDistance)
    loading: HashSet<(i32, i32)>,
}

impl PlayerChunkTracker {
    // vanilla PlayerTicketTracker(maxDistance) viewDistance 初始 0 再由配置设
    pub fn with_view_distance(max_distance: i32, view_distance: i32) -> Self {
        Self {
            players_per_chunk: HashMap::new(),
            levels: HashMap::new(),
            computed_levels: HashMap::new(),
            priority_queue: LeveledPriorityQueue::new((max_distance + 2) as usize),
            max_distance,
            view_distance,
            loading: HashSet::new(),
        }
    }

    // vanilla FixedPlayerDistanceChunkTracker 的 super(maxDistance + 2, ...)
    fn level_count(&self) -> i32 {
        self.max_distance + 2
    }

    // 默认不可达 照搬 chunks.defaultReturnValue(maxDistance + 2)
    fn get_level(&self, node: i64) -> i32 {
        self.levels
            .get(&node)
            .map(|&v| v as i32)
            .unwrap_or(self.max_distance + 2)
    }

    // 照搬 FixedPlayerDistanceChunkTracker.setLevel 超距就移除 不占内存
    fn set_level(&mut self, node: i64, level: i32) {
        let old = if level > self.max_distance {
            self.levels.remove(&node).map(|v| v as i32)
        } else {
            self.levels.insert(node, level as u8).map(|v| v as i32)
        };
        // 加载票挂摘 照搬 PlayerTicketTracker.onLevelChange 的跨界判定
        let (cx, cz) = unpack_chunk(node);
        let had = old.is_some_and(|l| l <= self.view_distance);
        let has = level <= self.view_distance;
        if had != has {
            if has {
                self.loading.insert((cx, cz));
            } else {
                self.loading.remove(&(cx, cz));
            }
        }
    }

    // 源就是玩家所在区块 照搬 getLevelFromSource
    fn get_level_from_source(&self, to: i64) -> i32 {
        let has_player = self
            .players_per_chunk
            .get(&to)
            .is_some_and(|players| !players.is_empty());
        if has_player { 0 } else { i32::MAX }
    }

    fn compute_level_from_neighbor(&self, from: i64, to: i64, from_level: i32) -> i32 {
        if from == SOURCE {
            self.get_level_from_source(to)
        } else {
            from_level + 1
        }
    }

    // 照搬 ChunkTracker.getComputedLevel 八邻域取最小 自己是虚拟源的入口
    fn get_computed_level(&self, node: i64, known_parent: i64, known_level_from_parent: i32) -> i32 {
        let mut computed_level = known_level_from_parent;
        let (x, z) = unpack_chunk(node);
        for dx in -1..=1 {
            for dz in -1..=1 {
                let neighbor = if dx == 0 && dz == 0 {
                    SOURCE
                } else {
                    pack_chunk(x + dx, z + dz)
                };
                if neighbor != known_parent {
                    let cost = self.compute_level_from_neighbor(neighbor, node, self.get_level(neighbor));
                    if computed_level > cost {
                        computed_level = cost;
                    }
                    if computed_level == 0 {
                        return 0;
                    }
                }
            }
        }
        computed_level
    }

    fn calculate_priority(&self, level: i32, computed_level: i32) -> usize {
        level.min(computed_level).min(self.level_count() - 1) as usize
    }

    // vanilla checkEdge(from, to, newLevelFrom, onlyDecreased)
    fn check_edge(&mut self, from: i64, to: i64, new_level_from: i32, only_decreased: bool) {
        let level_to = self.get_level(to);
        let old_computed_level = self
            .computed_levels
            .get(&to)
            .copied()
            .unwrap_or(NO_COMPUTED_LEVEL);
        self.check_edge_inner(from, to, new_level_from, level_to, old_computed_level, only_decreased);
    }

    fn check_edge_inner(
        &mut self,
        from: i64,
        to: i64,
        mut new_level_from: i32,
        level_to: i32,
        old_computed_level: u8,
        only_decreased: bool,
    ) {
        if self.is_source(to) {
            return;
        }
        let level_count = self.level_count();
        new_level_from = new_level_from.clamp(0, level_count - 1);
        let level_to = level_to.clamp(0, level_count - 1);
        let was_consistent = old_computed_level == NO_COMPUTED_LEVEL;
        let old_computed = if was_consistent {
            level_to
        } else {
            old_computed_level as i32
        };

        let new_computed = if only_decreased {
            old_computed.min(new_level_from)
        } else {
            self.get_computed_level(to, from, new_level_from)
                .clamp(0, level_count - 1)
        };

        let old_priority = self.calculate_priority(level_to, old_computed);
        if level_to != new_computed {
            let new_priority = self.calculate_priority(level_to, new_computed);
            if old_priority != new_priority && !was_consistent {
                self.priority_queue.dequeue(to, new_priority);
            }
            self.priority_queue.enqueue(to, new_priority);
            self.computed_levels.insert(to, new_computed as u8);
        } else if !was_consistent {
            self.priority_queue.remove_only(to);
            self.computed_levels.remove(&to);
        }
    }

    // vanilla checkNeighbor(from, to, level, onlyDecreased)
    fn check_neighbor(&mut self, from: i64, to: i64, level: i32, only_decreased: bool) {
        let stored_old_computed_level = self
            .computed_levels
            .get(&to)
            .copied()
            .unwrap_or(NO_COMPUTED_LEVEL);
        let level_from = self
            .compute_level_from_neighbor(from, to, level)
            .clamp(0, self.level_count() - 1);
        if only_decreased {
            let level_to = self.get_level(to);
            self.check_edge_inner(
                from,
                to,
                level_from,
                level_to,
                stored_old_computed_level,
                only_decreased,
            );
        } else {
            let was_consistent = stored_old_computed_level == NO_COMPUTED_LEVEL;
            let old_computed = if was_consistent {
                self.get_level(to).clamp(0, self.level_count() - 1)
            } else {
                stored_old_computed_level as i32
            };
            if level_from == old_computed {
                self.check_edge(from, to, self.level_count() - 1, false);
            }
        }
    }

    fn is_source(&self, node: i64) -> bool {
        node == SOURCE
    }

    // 照搬 ChunkTracker.checkNeighborsAfterUpdate 八邻域逐个复查
    fn check_neighbors_after_update(&mut self, node: i64, level: i32, only_decrease: bool) {
        if !only_decrease || level < self.level_count() - 2 {
            let (x, z) = unpack_chunk(node);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    let neighbor = pack_chunk(x + dx, z + dz);
                    if neighbor != node {
                        self.check_neighbor(node, neighbor, level, only_decrease);
                    }
                }
            }
        }
    }

    // vanilla FixedPlayerDistanceChunkTracker.runAllUpdates -> runUpdates(MAX)
    // 限量传播 剩余的下 tick 继续 单帧不会卡死
    pub fn run_updates(&mut self, max_updates: usize) -> bool {
        let mut count = max_updates;
        while !self.priority_queue.is_empty() && count > 0 {
            count -= 1;
            let node = self.priority_queue.remove_first_long();
            let level = self.get_level(node).clamp(0, self.level_count() - 1);
            let computed_level = self
                .computed_levels
                .remove(&node)
                .map(|v| v as i32)
                .unwrap_or(NO_COMPUTED_LEVEL as i32);
            if computed_level < level {
                self.set_level(node, computed_level);
                self.check_neighbors_after_update(node, computed_level, true);
            } else if computed_level > level {
                self.set_level(node, self.level_count() - 1);
                if computed_level != self.level_count() - 1 {
                    let priority =
                        self.calculate_priority(self.level_count() - 1, computed_level);
                    self.priority_queue.enqueue(node, priority);
                    self.computed_levels.insert(node, computed_level as u8);
                }
                self.check_neighbors_after_update(node, level, false);
            }
        }
        !self.priority_queue.is_empty()
    }

    // 距离场算出的加载票 手电筒照到的区块
    pub fn loading_chunks(&self) -> &HashSet<(i32, i32)> {
        &self.loading
    }

    // 照搬 DistanceManager.addPlayer
    pub fn add_player(&mut self, addr: SocketAddr, chunk: (i32, i32)) {
        let node = pack_chunk(chunk.0, chunk.1);
        self.players_per_chunk
            .entry(node)
            .or_default()
            .insert(addr);
        // 新增源 level 0 只降不升 vanilla update(node, 0, true)
        self.check_edge(SOURCE, node, 0, true);
    }

    // 照搬 DistanceManager.removePlayer 区块里没人了才撤源
    pub fn remove_player(&mut self, addr: SocketAddr, chunk: (i32, i32)) {
        let node = pack_chunk(chunk.0, chunk.1);
        let empty = if let Some(players) = self.players_per_chunk.get_mut(&node) {
            players.remove(&addr);
            players.is_empty()
        } else {
            false
        };
        if empty {
            self.players_per_chunk.remove(&node);
            // 撤源 level 抬到不可达 vanilla update(node, MAX, false)
            self.check_edge(SOURCE, node, self.level_count() - 1, false);
        }
    }

    // 照搬 ChunkMap.move 的时序 先摘旧区块的源 再挂新区块的源
    pub fn move_player(&mut self, addr: SocketAddr, from: (i32, i32), to: (i32, i32)) {
        if from == to {
            return;
        }
        self.remove_player(addr, from);
        self.add_player(addr, to);
    }

    // 照搬 PlayerTicketTracker.updateViewDistance 每个已记录区块重新判挂摘
    pub fn update_view_distance(&mut self, view_distance: i32) {
        let nodes: Vec<i64> = self.levels.keys().copied().collect();
        for node in nodes {
            let level = self.get_level(node);
            let (cx, cz) = unpack_chunk(node);
            let had = self.loading.contains(&(cx, cz));
            let has = level <= view_distance;
            if had != has {
                if has {
                    self.loading.insert((cx, cz));
                } else {
                    self.loading.remove(&(cx, cz));
                }
            }
        }
        self.view_distance = view_distance;
    }

    // 视距上限 建场时用
    pub fn max_distance(&self) -> i32 {
        self.max_distance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(n: u16) -> SocketAddr {
        format!("127.0.0.1:{n}").parse().unwrap()
    }

    fn tracker() -> PlayerChunkTracker {
        PlayerChunkTracker::with_view_distance(32, 2)
    }

    fn settle(t: &mut PlayerChunkTracker) {
        // 传播是限量的 一口气跑完所有脏节点
        while t.run_updates(usize::MAX) {}
    }

    // 单个玩家的手电筒是 Chebyshev 方形 level 就是切比雪夫距离
    #[test]
    fn single_player_beam_is_a_square() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        settle(&mut t);

        assert_eq!(t.get_level(pack_chunk(0, 0)), 0);
        assert_eq!(t.get_level(pack_chunk(1, 0)), 1);
        assert_eq!(t.get_level(pack_chunk(2, 2)), 2);
        assert_eq!(t.get_level(pack_chunk(3, 0)), 3);
        // 视距 2 的加载票是 5x5 25 块
        assert_eq!(t.loading_chunks().len(), 25);
        assert!(t.loading_chunks().contains(&(2, 2)));
        assert!(!t.loading_chunks().contains(&(3, 0)));
    }

    // 两个玩家靠近时加载圈合并 level 取最近源
    #[test]
    fn nearby_players_merge_beams() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        t.add_player(addr(50002), (10, 0));
        settle(&mut t);
        assert_eq!(t.loading_chunks().len(), 50);

        // 走近到 3 格外 两块圈合成一块 7x5
        t.move_player(addr(50002), (10, 0), (3, 0));
        settle(&mut t);
        assert!(t.loading_chunks().contains(&(5, 0)));
        assert!(t.loading_chunks().contains(&(1, 0)));
        // 合并后比两块独立圈少
        assert!(t.loading_chunks().len() < 50);
    }

    // 下线后手电筒熄灭 加载票清空
    #[test]
    fn leaving_player_dims_the_light() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        settle(&mut t);
        assert_eq!(t.loading_chunks().len(), 25);

        t.remove_player(addr(50001), (0, 0));
        settle(&mut t);
        assert!(t.loading_chunks().is_empty());
        assert!(t.levels.is_empty(), "不可达的 level 要移除 不占内存");
    }

    // 传播限量后还有残留 下批继续
    #[test]
    fn updates_are_throttled() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        let has_more = t.run_updates(1);
        assert!(has_more, "一次只跑一个节点 应该有残留");
        while t.run_updates(usize::MAX) {}
        assert!(!t.run_updates(usize::MAX));
        assert_eq!(t.loading_chunks().len(), 25);
    }

    // 同一个区块的多个玩家 一个下线不影响另一个
    #[test]
    fn players_in_same_chunk_are_refcounted() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        t.add_player(addr(50002), (0, 0));
        settle(&mut t);
        assert_eq!(t.loading_chunks().len(), 25);

        t.remove_player(addr(50001), (0, 0));
        settle(&mut t);
        assert_eq!(t.loading_chunks().len(), 25, "还有一个玩家在照");

        t.remove_player(addr(50002), (0, 0));
        settle(&mut t);
        assert!(t.loading_chunks().is_empty());
    }

    // 视距改小后加载圈收缩 照搬 updateViewDistance
    #[test]
    fn smaller_view_distance_shrinks_the_beam() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        settle(&mut t);
        assert_eq!(t.loading_chunks().len(), 25);

        t.update_view_distance(1);
        assert_eq!(t.loading_chunks().len(), 9);
        assert!(!t.loading_chunks().contains(&(2, 0)));

        t.update_view_distance(2);
        assert_eq!(t.loading_chunks().len(), 25);
    }

    // 玩家走出视距 旧区域的加载票被摘掉
    #[test]
    fn walking_away_releases_the_old_area() {
        let mut t = tracker();
        t.add_player(addr(50001), (0, 0));
        settle(&mut t);

        // 跨到 32 格外 旧 5x5 全部出圈
        t.move_player(addr(50001), (0, 0), (40, 0));
        settle(&mut t);
        assert!(!t.loading_chunks().contains(&(0, 0)));
        assert!(!t.loading_chunks().contains(&(2, 0)));
        assert!(t.loading_chunks().contains(&(40, 0)));
        assert_eq!(t.loading_chunks().len(), 25);
    }
}
