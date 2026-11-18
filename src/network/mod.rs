use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tracing::{info, warn};

pub use crate::config::{DEFAULT_VIEW_DISTANCE, MAX_VIEW_DISTANCE};
use crate::config::server::DEFAULT_MAX_PLAYERS;
use crate::error::Result;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::generator::Generator;
use crate::level::generator::terrain::SEA_LEVEL;
use crate::level::level::Level;
use crate::level::region::RegionId;
use crate::network::protocol::move_player::MovePlayerPacket;
use crate::command::{self, Command, CommandSay};
use crate::network::protocol::pid::{BATCH, LOGIN, MOVE_PLAYER, TEXT};
use crate::network::protocol::player_list::{PlayerListEntry, PlayerListPacket};
use crate::network::protocol::remove_player::RemovePlayerPacket;
use crate::network::protocol::text::{TYPE_CHAT, TextPacket};
use crate::raknet::GAME_PACKET_PREFIX;
use crate::tick::metrics::{RegionHealth, TickMeter, TickStats};
use crate::tick::os::{OsMeter, OsSample};

pub mod batch;
pub mod protocol;

mod broadcast;
mod handlers;
mod login;

use broadcast::reply;
use crate::entity::{ItemDrop, PlayerData, PlayerDirectory, PlayerSnapshot, PlayerState};
use crate::level::distance::PlayerChunkTracker;
use crate::level::WorldTime;
use login::spawn_sequence;

#[cfg(test)]
mod tests;

// 实体相关的常量都定义在 entity 层 这里转发一下 老代码与测试不用改引用路径
pub use crate::entity::{
    ITEM_HALF_HEIGHT, ITEM_LIFETIME, ITEM_PICKUP_DELAY, ITEM_PICKUP_GROW_HORIZONTAL,
    ITEM_PICKUP_GROW_VERTICAL, PLAYER_HALF_WIDTH, PLAYER_HEIGHT, SPAWN_POSITION,
};

// SendCmd / CommandOutput 的定义在 command 层 这里只做再导出
// 让 network 内外的既有引用(region_handler/broadcast/handlers/tests/tick_manager)无需改动
pub use crate::command::SendCmd;
pub(crate) use crate::command::CommandOutput;

pub const SPAWN_BLOCK: (i32, i32, i32) = (0, 4, 0);

pub const GAMEMODE_SURVIVAL: u8 = 0;
pub const GAMEMODE_CREATIVE: u8 = 1;
// 0.14 单个区块序列化后约 81KB 每 tick 分批下发 避免一次性打爆 UDP 缓冲
pub const CHUNKS_PER_TICK: usize = 4;
// 距离场每 tick 最多传播多少个节点 超了留给下 tick 单帧不卡死
const MAX_DISTANCE_UPDATES: usize = 4096;

pub struct NetworkState {
    players: HashMap<SocketAddr, PlayerState>,
    // 实体 id 从 1 开始 0 留给客户端自己的实体
    next_eid: i64,
    gamemode: u8,
    // 全服难度 /difficulty 可改 登录时下发给新玩家 默认值与 login 原硬编码一致
    difficulty: i32,
    level: Level,
    // 传给客户端的世界种子
    seed: u32,
    // 服务端允许的区块半径 越大一次进服要下发的区块越多
    view_distance: i32,
    // 玩家上限 来自 server.toml 的 max-players list 命令与 pong 共用
    max_players: u32,
    // 世界里现存的掉落物 由各 region 回报的只读镜像
    drops: Vec<ItemDrop>,
    drops_by_region: HashMap<RegionId, Vec<ItemDrop>>,
    // 全局玩家目录 global region 写 各 region 只读
    directory: Arc<PlayerDirectory>,
    // 玩家归属分片 包按这个表投递
    player_index: HashMap<SocketAddr, RegionId>,
    // 每个 UDP 端点当前生效的会话 id 同端点重连时新会话直接顶替旧会话
    // 旧会话的关闭/残留包带旧 id 会被忽略 不会误删或覆盖新玩家
    session_of: HashMap<SocketAddr, u64>,
    // 上一轮 GC 的根集合 只在变化时重新标记
    last_gc_roots: HashMap<RegionId, HashSet<(i32, i32)>>,
    // 玩家区块距离场 照搬 vanilla DistanceManager 手电筒照到哪加载到哪
    chunk_tracker: PlayerChunkTracker,
    gc_ticks: u64,
    // 出生点选点结果缓存 首次算完复用 避免每次登录都跨分片同步取列高
    spawn_cache: Option<((i32, i32, i32), (f32, f32, f32))>,
    // 服务端世界时间 照搬 Nukkit Level 的 checkTime/sendTime
    world_time: WorldTime,
    // stop 命令触发 主循环据此优雅停机
    shutdown_requested: bool,
    // 世界名称 来自 server.toml 的 world 用于玩家上下线日志
    world_name: String,
    // 容器(工作台/熔炉/箱子)内容与烧制进度 按方块坐标存 关窗不丢
    containers: crate::item::containers::ContainerRegistry,
    // 熔炉进度条同步节流 每 10 tick 一次
    furnace_sync_ticks: u32,
    // TPS/MSPT 采样 主循环每拍写 /tps 只读
    metrics: TickMeter,
    // 各分片每秒上报的健康快照 /tps 汇总展示
    region_health: HashMap<RegionId, RegionHealth>,
    // 系统与进程性能采样 /top 只读
    os: OsMeter,
    // OP 名单 按用户名判定(大小写不敏感) 由 op.txt 载入 /op /deop 维护
    ops: crate::permission::OpList,
}

impl NetworkState {
    // 启动时把 server.toml 的 max-players 灌进来 构造时先用默认值
    pub(crate) fn set_max_players(&mut self, max_players: u32) {
        self.max_players = max_players;
    }

    pub fn new(gamemode: u8) -> Self {
        Self::with_world(gamemode, Level::new(), 0, DEFAULT_VIEW_DISTANCE, "world".to_string())
    }

    // 带存档与地形生成器的世界 区块缺失时按生成器创建
    pub fn with_loader(
        gamemode: u8,
        loader: ChunkLoader,
        generator: Generator,
        seed: u32,
        view_distance: i32,
        io_workers: usize,
        world_name: String,
    ) -> Self {
        let mut level = Level::with_loader(loader, io_workers);
        level.set_generator(generator);
        Self::with_world(gamemode, level, seed, view_distance, world_name)
    }

    pub fn with_world(
        gamemode: u8,
        mut level: Level,
        seed: u32,
        view_distance: i32,
        world_name: String,
    ) -> Self {
        let view_distance = view_distance.clamp(1, MAX_VIEW_DISTANCE);
        level.set_view_distance(view_distance);
        let directory = level.directory();
        Self {
            players: HashMap::new(),
            next_eid: 1,
            gamemode,
            difficulty: crate::network::protocol::set_difficulty::NORMAL,
            level,
            seed,
            view_distance,
            max_players: DEFAULT_MAX_PLAYERS,
            drops: Vec::new(),
            drops_by_region: HashMap::new(),
            directory,
            player_index: HashMap::new(),
            session_of: HashMap::new(),
            last_gc_roots: HashMap::new(),
            chunk_tracker: PlayerChunkTracker::with_view_distance(MAX_VIEW_DISTANCE, view_distance),
            gc_ticks: 0,
            spawn_cache: None,
            world_time: WorldTime::default(),
            shutdown_requested: false,
            world_name,
            containers: crate::item::containers::ContainerRegistry::default(),
            furnace_sync_ticks: 0,
            metrics: TickMeter::new(),
            region_health: HashMap::new(),
            os: OsMeter::new(),
            ops: crate::permission::OpList::new(),
        }
    }

    // 从 op.txt 载入 OP 名单 启动时调用一次 控制台可无条件添加第一个 OP
    pub(crate) fn load_ops(&mut self, path: &str) {
        self.ops = crate::permission::OpList::load(path);
        tracing::info!("Loaded {} op(s) from {path}", self.ops.len());
    }

    // /top 读的快照 拷贝 cost 很小
    pub(crate) fn os_sample(&self) -> OsSample {
        self.os.sample()
    }

    // 主循环每拍调用 gap 是上一拍开始到这一拍开始的间隔 mspt 是这一拍干活耗时
    pub(crate) fn record_tick(&mut self, gap: Duration, mspt: Duration) {
        self.metrics.record(gap, mspt);
    }

    // 当前窗口的采样统计 一拍都还没跑完时是 None
    pub(crate) fn tick_stats(&self) -> Option<TickStats> {
        self.metrics.stats()
    }

    pub fn view_distance(&self) -> i32 {
        self.view_distance
    }

    // 出生点优先选海平面以上的陆地 附近没有就退回首列水面
    // 33x33 列的列高一次批量取回 逐列跨线程查询要几毫秒
    pub fn spawn_point(&mut self) -> ((i32, i32, i32), (f32, f32, f32)) {
        // 首次算完缓存 之后每次登录直接复用 不再跨分片同步取列高
        if let Some(cached) = self.spawn_cache {
            return cached;
        }
        let (sx, sz) = (SPAWN_BLOCK.0, SPAWN_BLOCK.2);
        let mut columns = Vec::with_capacity(33 * 33);
        for dx in -16..=16 {
            for dz in -16..=16 {
                columns.push((sx + dx, sz + dz));
            }
        }
        let heights = self.level.highest_blocks(&columns);

        let mut best = None;
        let mut best_distance = i32::MAX;
        let mut fallback_y = 1;
        for (index, (x, z)) in columns.iter().enumerate() {
            let y = heights[index] + 1;
            if *x == sx && *z == sz {
                fallback_y = y;
            }
            // 海平面往上两格以上才是陆地 水面那层不算
            if y <= SEA_LEVEL + 1 {
                continue;
            }
            let distance = (x - sx) * (x - sx) + (z - sz) * (z - sz);
            if distance < best_distance {
                best_distance = distance;
                best = Some((*x, y, *z));
            }
        }

        let (x, y, z) = best.unwrap_or((sx, fallback_y, sz));
        let result = ((x, y, z), (x as f32 + 0.5, y as f32, z as f32 + 0.5));
        self.spawn_cache = Some(result);
        result
    }

    pub fn block_at(&mut self, x: i32, y: i32, z: i32) -> u8 {
        self.level.block(x, y, z)
    }

    // 把改过的区块写回存档 返回写入数量 只写进 page cache 不刷盘
    pub fn flush_world(&mut self) -> Result<usize> {
        self.level.flush()
    }

    // 周期保存走异步 只投递不等 region 分批编码写盘 不阻塞 global tick
    pub fn flush_world_async(&mut self) {
        self.level.flush_async();
    }

    // 真正刷盘 停机时用
    pub fn sync_world(&self) -> Result<()> {
        self.level.sync()
    }

    // 后台刷盘线程用的存档句柄
    pub fn sync_handle(&self) -> Option<Arc<ChunkLoader>> {
        self.level.loader()
    }

    // 停机兜底 把在线玩家的最新状态全部写盘
    // 照搬 Nukkit Server.shutdown 的 saveOnlinePlayerData:
    // Ctrl+C 直接终止进程时不会走 SessionClosed 不兜底的话 players/ 永远不会出现
    pub fn save_all_players(&self) {
        let Some(dir) = self.level.data_dir().map(|p| p.to_path_buf()) else {
            return;
        };
        for state in self.players.values().filter(|s| s.logged_in) {
            let data = PlayerData::from_state(state);
            if let Err(e) = data.save(&dir, state.uuid) {
                warn!("保存玩家数据失败 {}: {e}", state.username);
            }
        }
    }

    // RakNet 握手完成 记录该端点当前生效的会话
    // 同端点重连时新会话直接顶替旧会话 旧会话收尾时靠 id 区分
    pub fn on_session_opened(&mut self, id: u64, addr: &SocketAddr) {
        self.session_of.insert(*addr, id);
    }

    // 事件是否来自当前生效的会话 测试里未登记会话时放行
    pub fn session_matches(&self, id: u64, addr: &SocketAddr) -> bool {
        self.session_of.get(addr).is_none_or(|current| *current == id)
    }

    // 下线时通知其他玩家 移除实体与玩家列表项
    pub fn on_session_closed(&mut self, id: u64, addr: &SocketAddr, reason: &str) -> Vec<SendCmd> {
        // 旧会话退役时新连接可能已经接管这个端点 不能按 addr 直接清掉新玩家
        if !self.session_matches(id, addr) {
            return Vec::new();
        }
        self.session_of.remove(addr);
        let Some(state) = self.players.remove(addr) else {
            return Vec::new();
        };
        // 下线必须解除容器关联 否则同一端口的会话重连后
        // 还会收到指向已关闭窗口的 ContainerSetSlot 客户端会崩
        self.containers.close(*addr);
        // 同一 uuid 的玩家已经用新端点重新上线 旧端点超时收尾时不能再写盘或广播
        // 否则会覆盖新会话刚写的存档 还会发出错误的 left the game
        // 同名但 uuid 不同是另一台设备上的另一个人 两份存档各写各的 不能当重连吞掉
        let reconnected = state.logged_in
            && self
                .players
                .values()
                .any(|other| other.logged_in && other.uuid == state.uuid);

        // 下线存档 照搬 Nukkit saveOfflinePlayerData(username)
        // 位置朝向背包按 uuid 写回 playerdata/<uuid>.dat 下次登录读回
        if let Some(dir) = self.level.data_dir().filter(|_| !reconnected) {
            let data = PlayerData::from_state(&state);
            if let Err(e) = data.save(dir, state.uuid) {
                warn!("保存玩家数据失败 {}: {e}", state.username);
            }
        }
        // 断开原因优先于是否进过世界 没进服也照常记一笔
        info!("{} lost connection: {}", state.username, reason);

        self.player_index.remove(addr);
        // 最后一个玩家退出就立刻撤掉全部分片 区块缓存与线程一起释放
        // 不等 2 秒空闲判定 退出即回收 尽快回到冷启动基线
        if self.players.is_empty() {
            self.level.remove_all_regions();
            // 分片都没了 掉落物镜像也一起清空 不留残余
            self.drops_by_region.clear();
            self.drops.clear();
        }
        // 把区块下发视图从所在 region 撤掉 没进过世界的不用撤
        if state.spawned || state.spawn_pending {
            let region = RegionId::of_block(
                state.position.0.floor() as i32,
                state.position.2.floor() as i32,
            );
            self.level.leave(*addr, region);
            // 手电筒熄灭 照搬 DistanceManager.removePlayer
            let chunk = (
                state.position.0.floor() as i32 >> 4,
                state.position.2.floor() as i32 >> 4,
            );
            self.chunk_tracker.remove_player(*addr, chunk);
        }
        // 新连接已经接管同名玩家 旧端点不再广播离开 也不记 left the game
        if reconnected || !state.spawned {
            return Vec::new();
        }
        // 进过世界才记 "left the game" 与游戏内广播对应
        info!("§6{} left the game", state.username);

        // 实体移除只给看得见他的人 列表项与离开提示全服都发
        let entity_gone = RemovePlayerPacket {
            eid: state.eid,
            uuid: *state.uuid.as_bytes(),
        }
        .encode();
        let radius = (self.view_distance * 16) as f32;
        let center = state.position;
        let list_gone = PlayerListPacket::remove(vec![*state.uuid.as_bytes()]).encode();
        let notice = TextPacket::raw(&format!("§e{} left the game", state.username)).encode();
        self.players
            .iter()
            .filter(|(_, peer)| peer.spawned)
            .map(|(peer, snapshot)| {
                let dx = snapshot.position.0 - center.0;
                let dy = snapshot.position.1 - center.1;
                let dz = snapshot.position.2 - center.2;
                let mut bodies = Vec::with_capacity(3);
                if dx * dx + dy * dy + dz * dz <= radius * radius {
                    bodies.push(entity_gone.clone());
                }
                bodies.push(list_gone.clone());
                bodies.push(notice.clone());
                reply(*peer, bodies)
            })
            .collect()
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    // stop 命令是否已被触发 主循环据此 break
    pub(crate) fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    #[cfg(test)]
    pub(crate) fn set_idle_ticks_for_test(&mut self, ticks: u64) {
        self.level.set_idle_ticks_for_test(ticks);
    }

    // 测试注入掉落物年龄 权威状态在分片里 只能投递过去改
    #[cfg(test)]
    pub(crate) fn set_drop_age_for_test(&mut self, age: u32) {
        let Some(drop) = self.drops.first() else {
            return;
        };
        let id = RegionId::of_block(
            drop.position.0.floor() as i32,
            drop.position.2.floor() as i32,
        );
        self.level.set_drop_age(id, 0, age);
    }

    // 业务 GC 的根 距离场里挂着加载票的区块就是所有玩家的手电筒照到的地方
    // 按 region 分组投给各分片 其余区块不可达
    fn gc_roots(&self) -> HashMap<RegionId, HashSet<(i32, i32)>> {
        let mut roots: HashMap<RegionId, HashSet<(i32, i32)>> = HashMap::new();
        for &(chunk_x, chunk_z) in self.chunk_tracker.loading_chunks() {
            roots
                .entry(RegionId::of_chunk(chunk_x, chunk_z))
                .or_default()
                .insert((chunk_x, chunk_z));
        }
        roots
    }

    // global region 一帧一次 刷新玩家目录 收各分片产出并展开下行
    // 世界 tick 都在分片线程里 这里不再算掉落物 也不再碰区块数据
    pub async fn tick(&mut self) -> Vec<SendCmd> {
        let tick_start = Instant::now();
        let mut out = Vec::new();

        // 服务端世界时间照搬 Nukkit Level.checkTime 与 200 tick 一次 sendTime
        if self.world_time.tick() {
            let bodies = self.world_time.time_packets();
            for peer in self.players.keys() {
                out.push(reply(*peer, bodies.clone()));
            }
        }

        // 玩家目录由 global region 独占写入 各分片只读这份快照
        let snapshots: Vec<(SocketAddr, PlayerSnapshot)> = self
            .players
            .iter()
            .map(|(addr, state)| (*addr, PlayerSnapshot::from_state(state)))
            .collect();
        self.directory.replace(snapshots);

        // 距离场增量传播 照搬 DistanceManager.runAllUpdates 的每 tick 驱动
        self.chunk_tracker.run_updates(MAX_DISTANCE_UPDATES);
        let after_updates = tick_start.elapsed();
        // 根变化就立刻重新标记 另外每秒无条件补一次
        // 补的这一次是给刚按需重建的分片 否则它收不到第一次标记
        self.gc_ticks += 1;
        let roots = self.gc_roots();
        if roots != self.last_gc_roots || self.gc_ticks.is_multiple_of(20) {
            self.level.run_gc(&roots);
            // 可达分片之外的存档句柄也按同一份根集合回收
            let live: HashSet<RegionId> = roots.keys().copied().collect();
            self.level.gc_files(&live);
            self.last_gc_roots = roots;
        }
        let after_gc = tick_start.elapsed();

        let outbounds = self.level.drain_outbound();
        for item in outbounds {
            // region 做完了世界改动 结果先落地
            for event in item.events {
                self.on_region_event(event, &mut out);
            }
            match item.to {
                Some(addr) => {
                    if !item.bodies.is_empty() {
                        out.push(reply(addr, item.bodies));
                    }
                    if item.chunks_complete {
                        self.on_chunks_complete(addr, &mut out);
                    }
                }
                None => {
                    if !item.bodies.is_empty() {
                        out.extend(self.expand_broadcast(&item.bodies, item.skip, item.around));
                    }
                }
            }
        }

        let after_drain = tick_start.elapsed();
        // 熔炉每 tick 烧制 槽位变化立即同步 进度条定期推给正在看这个容器的人
        out.extend(self.tick_containers());
        let total = tick_start.elapsed();

        // 慢帧诊断：仅在 network.tick 本身明显超时时打印，定位是哪一段拖慢。
        if total.as_millis() > 100 {
            warn!(
                total_ms = total.as_millis() as u64,
                updates_ms = after_updates.as_millis() as u64,
                gc_ms = after_gc.saturating_sub(after_updates).as_millis() as u64,
                drain_ms = after_drain.saturating_sub(after_gc).as_millis() as u64,
                containers_ms = total.saturating_sub(after_drain).as_millis() as u64,
                "慢帧诊断(network.tick)"
            );
        }

        out
    }

    // 照搬 Nukkit BlockEntityFurnace.onUpdate 每 tick 推进燃料与烧制
    // 只给打开该容器的玩家发进度 燃烧方块切换广播给视野内所有人
    fn tick_containers(&mut self) -> Vec<SendCmd> {
        use crate::item::containers::{KIND_FURNACE, SlotChange, slot_packet};
        use crate::network::protocol::container::ContainerSetSlotPacket;

        let mut out = Vec::new();
        let mut lit_changes: Vec<(i32, i32, i32, bool)> = Vec::new();
        self.furnace_sync_ticks = self.furnace_sync_ticks.wrapping_add(1);
        let sync_progress = self.furnace_sync_ticks.is_multiple_of(10);
        let manager = crate::item::inventory::CraftingManager::new();
        let addrs: Vec<SocketAddr> = self.containers.opened.keys().copied().collect();

        let positions: Vec<(i32, i32, i32)> = self
            .containers
            .blocks
            .iter()
            .filter(|(_, s)| s.kind == KIND_FURNACE)
            .map(|(p, _)| *p)
            .collect();

        for pos in positions {
            let Some(state) = self.containers.blocks.get_mut(&pos) else {
                continue;
            };
            let changes = state.tick(&manager);
            let window_id = state.window_id;
            let viewers: Vec<SocketAddr> = addrs
                .iter()
                .copied()
                .filter(|a| self.containers.pos_of(*a) == Some(pos))
                .collect();

            // 槽位变化(吃燃料/出产物/原料减少)立刻推给观察者
            for change in changes {
                let (slot, item) = match change {
                    SlotChange::Smelting(i) => (0, Some(i)),
                    SlotChange::Fuel(i) => (1, Some(i)),
                    SlotChange::Result(i) => (2, Some(i)),
                    SlotChange::Lit(lit) => {
                        lit_changes.push((pos.0, pos.1, pos.2, lit));
                        continue;
                    }
                };
                let clean = item.filter(|i| i.id > 0 && i.count > 0);
                let packet: ContainerSetSlotPacket = slot_packet(window_id, slot, clean);
                for addr in &viewers {
                    out.push(reply(*addr, vec![packet.encode()]));
                }
            }

            // 熔炉进度条 ContainerSetData 暂不下发 0.14 客户端收到会闪退
            if sync_progress {
                // 保留节流计数 之后换别的方式同步烧制进度时直接用
            }
        }

        // 点燃/熄火: 服务端世界也要切成 61/62 否则客户端重载区块会变回去
        for (x, y, z, lit) in lit_changes {
            self.level.set_furnace_lit(x, y, z, lit);
            let meta = self
                .containers
                .blocks
                .get(&(x, y, z))
                .map(|s| s.meta)
                .unwrap_or(0);
            let id = if lit {
                crate::level::generator::blocks::BURNING_FURNACE
            } else {
                crate::level::generator::blocks::FURNACE
            };
            out.extend(self.broadcast_block(x, y, z, id, meta));
        }
        out
    }

    // 广播展开按玩家目录来 skip 用于拾取包不发给拾取者
    // around 是分片给的范围 只投给视野内的玩家 None 表示全服
    fn expand_broadcast(
        &self,
        bodies: &[Bytes],
        skip: Option<SocketAddr>,
        around: Option<((f32, f32, f32), f32)>,
    ) -> Vec<SendCmd> {
        let viewers = match around {
            Some((center, radius)) => self.directory.nearby(center, radius),
            None => self.directory.spawned(),
        };
        viewers
            .into_iter()
            .filter(|(addr, _)| Some(*addr) != skip)
            .map(|(addr, _)| reply(addr, bodies.to_vec()))
            .collect()
    }

    // region 把某个玩家的区块队列发完了 这时才补出生状态
    fn on_chunks_complete(&mut self, addr: SocketAddr, out: &mut Vec<SendCmd>) {
        let position = {
            let Some(state) = self.players.get_mut(&addr) else {
                return;
            };
            if !state.spawn_pending {
                return;
            }
            state.spawn_pending = false;
            state.spawned = true;
            state.position
        };

        // 玩家列表必须先于出生包 照搬 Nukkit doFirstSpawn 的次序
        // 0.14 客户端收到 PLAYER_SPAWN 时会按列表里自己的条目构建实体
        // 顺序反了会因为找不到条目而一直停在加载界面
        out.extend(self.on_player_spawned(addr));
        let spawn = spawn_sequence(position, self.gamemode);
        out.push(reply(addr, spawn));
    }

    // 新玩家出生后 与自己和其他玩家互换玩家列表项与实体
    // 列表项全服同步 Tab 列表要完整 实体只互换视野内的
    fn on_player_spawned(&mut self, addr: SocketAddr) -> Vec<SendCmd> {
        let Some(me) = self.players.get(&addr) else {
            return Vec::new();
        };
        info!("UUID of player {} is {}",
            me.username, me.uuid
        );
        info!("{}[/{}] logged in with entity id {} at ([{}]{:.1}, {:.1}, {:.1})",
        me.username, addr, me.eid, self.world_name, me.position.0, me.position.1, me.position.2
        );
        let my_entry = me.list_entry();
        let my_add = me.add_packet();
        let radius = (self.view_distance * 16) as f32;
        let center = me.position;

        let others: Vec<(SocketAddr, PlayerListEntry, Bytes, bool)> = self
            .players
            .iter()
            .filter(|(peer, state)| **peer != addr && state.spawned)
            .map(|(peer, state)| {
                let dx = state.position.0 - center.0;
                let dy = state.position.1 - center.1;
                let dz = state.position.2 - center.2;
                let visible = dx * dx + dy * dy + dz * dz <= radius * radius;
                (*peer, state.list_entry(), state.add_packet(), visible)
            })
            .collect();

        let joined = TextPacket::raw(&format!("§e{} joined the game", me.username)).encode();
        let mut bodies = vec![PlayerListPacket::add(vec![my_entry.clone()]).encode()];
        for (_, entry, add, visible) in &others {
            bodies.push(PlayerListPacket::add(vec![entry.clone()]).encode());
            if *visible {
                bodies.push(add.clone());
            }
        }
        bodies.push(joined.clone());
        let mut out = vec![reply(addr, bodies)];
        self.level.send_drops(addr);

        // 同一份广播发给其他玩家 不再重复构造
        for (peer, _, _, visible) in &others {
            let mut bodies = vec![PlayerListPacket::add(vec![my_entry.clone()]).encode()];
            if *visible {
                bodies.push(my_add.clone());
            }
            bodies.push(joined.clone());
            out.push(reply(*peer, bodies));
        }
        out
    }

    // 处理一个完整的游戏包 返回需要回送的命令
    pub fn handle_game_packet(&mut self, addr: SocketAddr, payload: &[u8]) -> Vec<SendCmd> {
        let mut out = Vec::new();
        if payload.len() < 2 || payload[0] != GAME_PACKET_PREFIX {
            return out;
        }

        if payload[1] == BATCH {
            match batch::decode(&payload[2..]) {
                Ok(packets) => {
                    for packet in &packets {
                        self.dispatch(addr, packet, &mut out);
                    }
                }
                Err(_) => {},
            }
        } else {
            self.dispatch(addr, payload, &mut out);
        }
        out
    }

    fn dispatch(&mut self, addr: SocketAddr, packet: &[u8], out: &mut Vec<SendCmd>) {
        if packet.len() < 2 || packet[0] != GAME_PACKET_PREFIX {
            return;
        }

        // 登录由 global region 引导 移动要维护归属表 其余包全部投给属主分片消费
        match packet[1] {
            LOGIN => {
                self.on_login(addr, packet, out);
                return;
            }
            MOVE_PLAYER => {
                // 记录最新位置与朝向(供下线存档) 同时做跨区块路由
                if let Ok(mv) = MovePlayerPacket::decode(&packet[1..]) {
                    let position = mv.feet_position();
                    self.level.ensure_view(position, self.view_distance);
                    // 手电筒跟着人走 跨区块就迁移距离场的源 照搬 ChunkMap.move
                    let new_chunk = (
                        position.0.floor() as i32 >> 4,
                        position.2.floor() as i32 >> 4,
                    );
                    let old = self.players.get(&addr).map(|s| {
                        (
                            (s.position.0.floor() as i32) >> 4,
                            (s.position.2.floor() as i32) >> 4,
                        )
                    });
                    if let Some(old) = old
                        && old != new_chunk
                    {
                        self.chunk_tracker.move_player(addr, old, new_chunk);
                    }
                    // global 侧只保留最新快照 真正的物理模拟仍在 region 里
                    if let Some(state) = self.players.get_mut(&addr) {
                        state.position = position;
                        state.yaw = mv.yaw;
                        state.pitch = mv.pitch;
                    }
                }
            }
            TEXT => {
                // 斜杠命令在 global 层消费 不投分片也不进聊天广播
                if let Ok(text) = TextPacket::decode(&packet[1..])
                    && text.kind == TYPE_CHAT
                {
                    let message = text.message.trim();
                    if message.starts_with('/') {
                        let output = self.run_command_from(message, Some(addr));
                        for line in &output.messages {
                            out.push(reply(addr, vec![TextPacket::raw(line).encode()]));
                        }
                        out.extend(output.sync);
                        return;
                    }
                }
            }
            _ => {}
        }

        let Some(&id) = self.player_index.get(&addr) else {
            return;
        };
        let result = self
            .level
            .handle_packet(addr, id, packet, self.gamemode, self.view_distance);
        // 分片改过的玩家状态回写只读镜像
        for (peer, state) in result.players {
            self.players.insert(peer, state);
        }
        out.extend(result.commands);
    }

    // 命令执行与发送者解耦 玩家回文本包 控制台打印到终端
    // 时间类照搬 Nukkit TimeCommand 语义 权限广播暂缺
    pub(crate) fn run_command(&mut self, input: &str) -> CommandOutput {
        // 控制台发送者是 "Server"
        self.run_command_from(input, None)
    }

    // sender 是发起命令的玩家 控制台传 None
    // /say 这类广播命令需要知道是谁发的 才能照 Nukkit 填上显示名
    pub(crate) fn run_command_from(
        &mut self,
        input: &str,
        sender: Option<SocketAddr>,
    ) -> CommandOutput {
        // 控制台不带斜杠也能用 统一补上再解析
        let input = input.trim();
        let normalized = if input.starts_with('/') {
            input.to_string()
        } else {
            format!("/{input}")
        };
        let Some(parsed) = command::parse(&normalized) else {
            return CommandOutput {
                messages: Vec::new(),
                sync: Vec::new(),
            };
        };
        // 统一鉴权: OP 专属命令仅控制台或名单内玩家可用
        // 控制台 sender=None 永远放行 保证空名单时也能用 op 加人
        if command::permission_of(&parsed) == command::Permission::Op {
            let allowed = match sender {
                None => true,
                Some(addr) => self
                    .players
                    .get(&addr)
                    .map(|state| self.ops.is_op(&state.username))
                    .unwrap_or(false),
            };
            if !allowed {
                return CommandOutput {
                    messages: vec!["§cYou are not allowed to use this command".to_string()],
                    sync: Vec::new(),
                };
            }
        }
        let mut messages = Vec::new();
        let mut sync = Vec::new();
        match parsed {
            Command::Help(_) => messages.extend(command::CommandHelp::Help.execute()),
            Command::List(_) => messages.extend(
                command::CommandList::List.execute(&self.players, self.max_players),
            ),
            Command::Say(cmd) => {
                let name = sender
                    .and_then(|addr| self.players.get(&addr))
                    .map(|state| state.username.as_str())
                    .unwrap_or("Server");
                let output = cmd.execute(name, &self.players);
                sync.extend(output.sync);
                // 广播包已经发给全体玩家(含发起者) 玩家侧不再单独回一条
                // 只有控制台和用法提示需要回 messages
                if sender.is_none() || matches!(cmd, CommandSay::Usage) {
                    messages.extend(output.messages);
                }
            }
            Command::Tell(cmd) => {
                let name = sender
                    .and_then(|addr| self.players.get(&addr))
                    .map(|state| state.username.as_str())
                    .unwrap_or("Server");
                let output = cmd.execute(name, &self.players);
                sync.extend(output.sync);
                // 私聊回执始终发给发送者(玩家回客户端 控制台打终端) 不进全服广播
                messages.extend(output.messages);
            }
            Command::Seed(_) => messages.extend(command::CommandSeed::Seed.execute(self.seed)),
            Command::Time(cmd) => {
                let output = cmd.execute(&mut self.world_time, &self.players);
                messages.extend(output.messages);
                sync.extend(output.sync);
            }
            Command::Teleport(cmd) => {
                let outcome = cmd.resolve(sender, &self.players);
                messages.extend(outcome.messages);
                for mv in outcome.moves {
                    // 镜像先跟上 同一个 tick 后面的包处理看到的就是新位置
                    if let Some(state) = self.players.get_mut(&mv.addr) {
                        state.position = mv.position;
                        state.yaw = mv.yaw;
                        state.pitch = mv.pitch;
                    }
                    // 落点视野跨到的分片要先建出来 Transfer 才有接收方
                    self.level.ensure_view(mv.position, self.view_distance);
                    if let Some(&id) = self.player_index.get(&mv.addr) {
                        self.level.teleport(mv.addr, id, mv.position, mv.yaw, mv.pitch);
                    }
                }
            }
            Command::Tps(_) => {
                let online = self.players.values().filter(|state| state.spawned).count();
                let regions: Vec<RegionHealth> = self.region_health.values().copied().collect();
                let report =
                    command::CommandTps::Tps.execute(self.tick_stats(), &regions, online, &self.world_name);
                messages.extend(report);
            }
            Command::Top(_) => {
                let sample = self.os_sample();
                messages.extend(command::TopCommand::Top.execute(&sample, self.tick_stats()));
            }
            Command::GameMode(cmd) => match cmd {
                crate::command::CommandGameMode::Usage => {
                    messages.push(command::gamemode::GAMEMODE_USAGE.to_string());
                }
                crate::command::CommandGameMode::Set { mode } => {
                    // 全局模式切换: 更新服务端状态 后续句柄据此判断能否破坏/飞行
                    self.gamemode = mode;
                    let bodies = vec![
                        crate::network::protocol::set_player_gametype::SetPlayerGameTypePacket {
                            gamemode: (mode & GAMEMODE_CREATIVE) as i32,
                        }
                        .encode(),
                        // 创造放开飞行 生存收回 与登录时的权限一致
                        crate::network::login::adventure_settings(mode).encode(),
                    ];
                    for peer in self.players.keys() {
                        sync.push(SendCmd::GamePackets {
                            addr: *peer,
                            bodies: bodies.clone(),
                        });
                    }
                    messages.push(command::CommandGameMode::success_message(mode));
                }
            },
            Command::Difficulty(cmd) => match cmd {
                crate::command::CommandDifficulty::Usage => {
                    messages.push(command::difficulty::DIFFICULTY_USAGE.to_string());
                }
                crate::command::CommandDifficulty::Set { difficulty } => {
                    self.difficulty = difficulty;
                    let body =
                        crate::network::protocol::set_difficulty::SetDifficultyPacket { difficulty }
                            .encode();
                    for peer in self.players.keys() {
                        sync.push(SendCmd::GamePackets {
                            addr: *peer,
                            bodies: vec![body.clone()],
                        });
                    }
                    messages.push(command::CommandDifficulty::success_message(difficulty));
                }
            },
            Command::Op(managed) => match managed {
                crate::command::CommandOp::Set { name, op } => {
                    let changed = self.ops.set(&name, op);
                    messages.push(crate::command::CommandOp::set_message(&name, op, changed));
                }
                crate::command::CommandOp::List => {
                    let names: Vec<&str> = self.ops.names_sorted();
                    messages.extend(crate::command::CommandOp::list_message(&names));
                }
                crate::command::CommandOp::Usage(cmd) => {
                    messages.push(crate::command::CommandOp::usage(cmd));
                }
            },
            Command::Version(op) => messages.extend(op.lines().map(str::to_string)),
            Command::Stop(_) => {
                self.shutdown_requested = true;
                messages.push(command::stop::CommandStop::Stop.message().to_string());
            }
            Command::Unknown(name) => {
                messages.push(if name.starts_with('/') {
                    name
                } else {
                    format!("Unknown command: /{name}")
                });
            }
        }
        CommandOutput { messages, sync }
    }

}

