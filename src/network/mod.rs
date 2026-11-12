use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use tracing::warn;

pub use crate::config::{DEFAULT_VIEW_DISTANCE, MAX_VIEW_DISTANCE};
use crate::error::Result;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::generator::Generator;
use crate::level::generator::terrain::SEA_LEVEL;
use crate::level::level::Level;
use crate::level::region::RegionId;
use crate::network::protocol::move_player::MovePlayerPacket;
use crate::command::{self, Command, TimeOp};
use crate::network::protocol::pid::{BATCH, LOGIN, MOVE_PLAYER, TEXT};
use crate::network::protocol::player_list::{PlayerListEntry, PlayerListPacket};
use crate::network::protocol::remove_player::RemovePlayerPacket;
use crate::network::protocol::text::{TYPE_CHAT, TextPacket};
use crate::raknet::GAME_PACKET_PREFIX;

pub mod batch;
pub mod protocol;

mod broadcast;
mod handlers;
mod login;
mod world_time;

use broadcast::reply;
use world_time::WorldTime;
use crate::entity::{ItemDrop, PlayerDirectory, PlayerSnapshot, PlayerState};
use crate::level::distance::PlayerChunkTracker;
use login::spawn_sequence;

#[cfg(test)]
mod tests;

// 实体相关的常量都定义在 entity 层 这里转发一下 老代码与测试不用改引用路径
pub use crate::entity::{
    ITEM_HALF_HEIGHT, ITEM_LIFETIME, ITEM_PICKUP_DELAY, ITEM_PICKUP_GROW_HORIZONTAL,
    ITEM_PICKUP_GROW_VERTICAL, PLAYER_HALF_WIDTH, PLAYER_HEIGHT, SPAWN_POSITION,
};

pub const SPAWN_BLOCK: (i32, i32, i32) = (0, 4, 0);

pub const GAMEMODE_SURVIVAL: u8 = 0;
pub const GAMEMODE_CREATIVE: u8 = 1;
// 0.14 单个区块序列化后约 81KB 每 tick 分批下发 避免一次性打爆 UDP 缓冲
pub const CHUNKS_PER_TICK: usize = 4;
// 距离场每 tick 最多传播多少个节点 超了留给下 tick 单帧不卡死
const MAX_DISTANCE_UPDATES: usize = 4096;

// 主循环发给 RakNet 层的下行命令
// 只带游戏包包体 0x8e 前缀由 RakNet 层封装时补 避免多一次整包拷贝
#[derive(Debug)]
pub enum SendCmd {
    GamePackets { addr: SocketAddr, bodies: Vec<Bytes> },
}

pub struct NetworkState {
    players: HashMap<SocketAddr, PlayerState>,
    // 实体 id 从 1 开始 0 留给客户端自己的实体
    next_eid: i64,
    gamemode: u8,
    level: Level,
    // 传给客户端的世界种子
    seed: u32,
    // 服务端允许的区块半径 越大一次进服要下发的区块越多
    view_distance: i32,
    // 世界里现存的掉落物 由各 region 回报的只读镜像
    drops: Vec<ItemDrop>,
    drops_by_region: HashMap<RegionId, Vec<ItemDrop>>,
    // 全局玩家目录 global region 写 各 region 只读
    directory: Arc<PlayerDirectory>,
    // 玩家归属分片 包按这个表投递
    player_index: HashMap<SocketAddr, RegionId>,
    // 上一轮 GC 的根集合 只在变化时重新标记
    last_gc_roots: HashMap<RegionId, HashSet<(i32, i32)>>,
    // 玩家区块距离场 照搬 vanilla DistanceManager 手电筒照到哪加载到哪
    chunk_tracker: PlayerChunkTracker,
    gc_ticks: u64,
    // 出生点选点结果缓存 首次算完复用 避免每次登录都跨分片同步取列高
    spawn_cache: Option<((i32, i32, i32), (f32, f32, f32))>,
    // 服务端世界时间 照搬 Nukkit Level 的 checkTime/sendTime
    world_time: WorldTime,
}

impl NetworkState {
    pub fn new(gamemode: u8) -> Self {
        Self::with_world(gamemode, Level::new(), 0, DEFAULT_VIEW_DISTANCE)
    }

    // 带存档与地形生成器的世界 区块缺失时按生成器创建
    pub fn with_loader(
        gamemode: u8,
        loader: ChunkLoader,
        generator: Generator,
        seed: u32,
        view_distance: i32,
        io_workers: usize,
    ) -> Self {
        let mut level = Level::with_loader(loader, io_workers);
        level.set_generator(generator);
        Self::with_world(gamemode, level, seed, view_distance)
    }

    pub fn with_world(gamemode: u8, mut level: Level, seed: u32, view_distance: i32) -> Self {
        let view_distance = view_distance.clamp(1, MAX_VIEW_DISTANCE);
        level.set_view_distance(view_distance);
        let directory = level.directory();
        Self {
            players: HashMap::new(),
            next_eid: 1,
            gamemode,
            level,
            seed,
            view_distance,
            drops: Vec::new(),
            drops_by_region: HashMap::new(),
            directory,
            player_index: HashMap::new(),
            last_gc_roots: HashMap::new(),
            chunk_tracker: PlayerChunkTracker::with_view_distance(MAX_VIEW_DISTANCE, view_distance),
            gc_ticks: 0,
            spawn_cache: None,
            world_time: WorldTime::default(),
        }
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

    // 下线时通知其他玩家 移除实体与玩家列表项
    pub fn on_session_closed(&mut self, addr: &SocketAddr) -> Vec<SendCmd> {
        let Some(state) = self.players.remove(addr) else {
            return Vec::new();
        };
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
        if !state.spawned {
            return Vec::new();
        }

        // 实体移除只给看得见他的人 列表项与离开提示全服都发
        let entity_gone = RemovePlayerPacket {
            eid: state.eid,
            uuid: state.uuid,
        }
        .encode();
        let radius = (self.view_distance * 16) as f32;
        let center = state.position;
        let list_gone = PlayerListPacket::remove(vec![state.uuid]).encode();
        let notice = TextPacket::raw(&format!("{} left the game", state.username)).encode();
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

        out.push(reply(addr, spawn_sequence(position)));
        out.extend(self.on_player_spawned(addr));
    }

    // 新玩家出生后 与自己和其他玩家互换玩家列表项与实体
    // 列表项全服同步 Tab 列表要完整 实体只互换视野内的
    fn on_player_spawned(&mut self, addr: SocketAddr) -> Vec<SendCmd> {
        let Some(me) = self.players.get(&addr) else {
            return Vec::new();
        };
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

        let mut bodies = vec![PlayerListPacket::add(vec![my_entry.clone()]).encode()];
        for (_, entry, add, visible) in &others {
            bodies.push(PlayerListPacket::add(vec![entry.clone()]).encode());
            if *visible {
                bodies.push(add.clone());
            }
        }
        let mut out = vec![reply(addr, bodies)];

        // 世界里已经存在的掉落物由各分片各自补发给新出生玩家
        self.level.send_drops(addr);

        let joined = TextPacket::raw(&format!("{} joined the game", me.username)).encode();
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
                Err(e) => warn!(%addr, error = %e, "批处理包解压失败"),
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
                // 只为路由 不改状态 跨区时先把目标视野涉及的分片建好
                if let Ok(mv) = MovePlayerPacket::decode(&packet[1..]) {
                    let position = mv.feet_position();
                    self.level.ensure_view(position, self.view_distance);
                    // 手电筒跟着人走 跨区块就迁移距离场的源 照搬 ChunkMap.move
                    let new_chunk = (
                        position.0.floor() as i32 >> 4,
                        position.2.floor() as i32 >> 4,
                    );
                    if let Some(old) = self
                        .players
                        .get(&addr)
                        .map(|s| ((s.position.0.floor() as i32) >> 4, (s.position.2.floor() as i32) >> 4))
                        && old != new_chunk
                    {
                        self.chunk_tracker.move_player(addr, old, new_chunk);
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
                        self.execute_command(addr, message, out);
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

    // 斜杠命令在 global 层直接执行 时间类照搬 Nukkit TimeCommand 语义
    // Nukkit 的 Command.broadcastCommandMessage 需要权限系统 这里先只回给发起者
    fn execute_command(&mut self, addr: SocketAddr, input: &str, out: &mut Vec<SendCmd>) {
        let Some(parsed) = command::parse(input) else {
            return;
        };
        match parsed {
            Command::Help => {
                for line in command::help_lines() {
                    out.push(reply(addr, vec![TextPacket::raw(line).encode()]));
                }
            }
            Command::List => {
                let names: Vec<&str> = self
                    .players
                    .values()
                    .filter(|state| state.spawned)
                    .map(|state| state.username.as_str())
                    .collect();
                let message = format!("在线玩家 ({}): {}", names.len(), names.join(", "));
                out.push(reply(addr, vec![TextPacket::raw(&message).encode()]));
            }
            Command::Seed => {
                let message = format!("Seed: {}", self.seed);
                out.push(reply(addr, vec![TextPacket::raw(&message).encode()]));
            }
            Command::Time(op) => {
                let message = match op {
                    TimeOp::Start => {
                        self.world_time.start_time();
                        "Restarted the time".to_string()
                    }
                    TimeOp::Stop => {
                        self.world_time.stop_time();
                        "Stopped the time".to_string()
                    }
                    TimeOp::Query => format!("Time is {}", self.world_time.time()),
                    TimeOp::Set(value) => {
                        let value = value.resolve();
                        self.world_time.set_time(value);
                        format!("Set the time to {value}")
                    }
                    TimeOp::Add(value) => {
                        let now = self.world_time.time();
                        self.world_time.set_time(now + value);
                        format!("Added {value} to the time")
                    }
                };
                // set/add/start/stop 立即同步 照搬 setTime/startTime/stopTime 里的 sendTime
                if !matches!(op, TimeOp::Query) {
                    let bodies = self.world_time.time_packets();
                    for peer in self.players.keys() {
                        out.push(reply(*peer, bodies.clone()));
                    }
                }
                out.push(reply(addr, vec![TextPacket::raw(&message).encode()]));
            }
            Command::Version(op) => {
                out.push(reply(addr, vec![TextPacket::raw(op.message()).encode()]));
            }
            Command::Unknown(name) => {
                let message = if name.starts_with('/') {
                    name
                } else {
                    format!("Unknown command: /{name}")
                };
                out.push(reply(addr, vec![TextPacket::raw(&message).encode()]));
            }
        }
    }

}

