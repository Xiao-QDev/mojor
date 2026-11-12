// Folia 式 region 线程 一个 region 一个执行单元 区块状态完全由它独占
// 区块的读写只在 region 线程里发生 外部一律通过消息进出 不存在共享可变状态
//
// 铁律：region 线程内部不允许再同步调用别的 RegionHandle
// 那样会形成 A 等 B、B 等 A 的死锁 跨 region 只能投递消息不做同步等待
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bytes::Bytes;
use tracing::warn;

use crate::entity::{ItemDrop, PlayerDirectory, RegionPlayer};
use crate::entity::{ITEM_LIFETIME, ITEM_PICKUP_DELAY};
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::item_entity::TakeItemEntityPacket;
use crate::network::protocol::remove_entity::RemoveEntityPacket;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::chunk_io::ChunkIoPool;
use crate::level::chunk_manager::ChunkManager;
use crate::level::format::chunk::Chunk;
use crate::level::generator::Generator;
use crate::level::region::RegionId;
use crate::level::region_handler::{self, PacketResult};
use crate::network::protocol::full_chunk_data::{FullChunkDataPacket, ORDER_COLUMNS};

// 每个 region 自己的 tick 周期 与主循环一致
pub const REGION_TICK: Duration = Duration::from_millis(50);
// 每个玩家每 tick 最多下发多少个区块 与网络层保持一致
const CHUNKS_PER_TICK: usize = 4;
// 连续这么多 tick 不可达就上报空闲 40 tick 也就是 2 秒
pub const IDLE_SHUTDOWN_TICKS: u64 = 40;
// 每 tick 最多编码写出多少脏块 16 块约 1.3MB 分摊进 50ms 不会被察觉
const MAX_FLUSH_PER_TICK: usize = 16;

// region 注册表 让 region 线程之间互相找得到
// 只存消息通道 拿到就能投递 不用拿到整个句柄
pub(crate) struct RegionRegistry {
    inner: Mutex<HashMap<RegionId, Sender<RegionMsg>>>,
}

impl RegionRegistry {
    pub(crate) fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn register(&self, id: RegionId, tx: Sender<RegionMsg>) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id, tx);
    }

    // 找某个分片的投递通道 分片还没建起来就返回 None
    pub(crate) fn lookup(&self, id: RegionId) -> Option<Sender<RegionMsg>> {
        let map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&id).cloned()
    }

    // 分片撤销后摘掉投递通道 免得别的分片拿到失效的 Sender
    pub(crate) fn unregister(&self, id: RegionId) {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&id);
    }
}

// region 做完世界改动后交给 global region 的事件
pub enum RegionEvent {
    // 挖掉一格 id 为 None 表示那里本来是空气
    BlockBroken {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
        id: Option<u8>,
    },
    // 放置一格 ok 为 false 表示目标格被占 没有真的放下去
    BlockPlaced {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        ok: bool,
    },
    // 掉落物被拾取 global region 据此加背包并刷新界面
    Pickup {
        addr: SocketAddr,
        item: crate::network::protocol::item::ItemStack,
    },
    // 本 region 掉落物快照 global region 按分片聚合出只读镜像
    DropsSnapshot {
        region: RegionId,
        drops: Vec<crate::entity::ItemDrop>,
    },
    // 本分片长期没有玩家和实体 global region 据此撤掉线程
    RegionIdle {
        region: RegionId,
    },
    // 玩家跨分片迁移完成 global region 据此更新归属表与镜像
    PlayerRegion {
        addr: SocketAddr,
        region: RegionId,
        state: Box<crate::entity::PlayerState>,
    },
}

// region 线程产出的下行数据 由 global region 取走展开
pub struct RegionOutbound {
    // None 表示广播给所有已出生玩家
    pub to: Option<SocketAddr>,
    // 广播时排除谁 拾取包要用
    pub skip: Option<SocketAddr>,
    // 广播范围 Some((中心, 半径)) 只投给视野内的玩家 None 表示全服
    pub around: Option<((f32, f32, f32), f32)>,
    pub bodies: Vec<Bytes>,
    // 世界改动的事件 只有 global region 关心
    pub events: Vec<RegionEvent>,
    // 该玩家的区块队列刚清空 网络层据此补出生状态
    pub chunks_complete: bool,
}

#[warn(private_interfaces)]
pub(crate) enum RegionMsg {
    GetBlock {
        x: i32,
        y: i32,
        z: i32,
        reply: Sender<u8>,
    },
    // 只读缓存 未命中回 None 不在 region 线程上同步读盘
    Payload {
        chunk_x: i32,
        chunk_z: i32,
        reply: Sender<Option<Bytes>>,
    },
    HighestBlock {
        x: i32,
        z: i32,
        reply: Sender<i32>,
    },
    HighestBlocks {
        columns: Vec<(i32, i32)>,
        reply: Sender<Vec<i32>>,
    },
    Flush {
        reply: Sender<usize>,
    },
    // 异步落盘请求 不等回执 region 在后续 tick 里分批写完
    FlushAsync,
    SetGenerator {
        generator: Generator,
        reply: Sender<()>,
    },
    Len {
        reply: Sender<usize>,
    },
    // 本 region 已经跑了多少个 tick 用于验证独立 tick 循环
    TickCount {
        reply: Sender<u64>,
    },
    // 玩家的区块下发视图归属本 region 之后 队列由本线程独占
    Join {
        addr: SocketAddr,
        player: Box<RegionPlayer>,
    },
    Leave {
        addr: SocketAddr,
        reply: Sender<()>,
    },
    // 别的分片请求我生成并直接下发给玩家
    // 我交付完回一条 ForeignDone 让请求方结算
    ForeignChunk {
        addr: SocketAddr,
        chunks: Vec<(i32, i32)>,
        ack: RegionId,
    },
    // 这批跨界区块已经交付 请求方可以结算
    ForeignDone {
        addr: SocketAddr,
        count: usize,
    },
    // global region 改过玩家状态后回写 权威仍在分片
    SyncPlayer {
        addr: SocketAddr,
        state: Box<crate::entity::PlayerState>,
    },
    // global region 的 GC 标记结果 本分片可达的区块
    Gc {
        live_chunks: HashSet<(i32, i32)>,
    },
    // 跨分片迁移 由源分片投递 目标分片接管玩家并重建视野
    Transfer {
        addr: SocketAddr,
        player: Box<RegionPlayer>,
        view_distance: i32,
    },
    // 玩家包交给本分片消费 global region 只等一份下行与玩家状态快照
    HandlePacket {
        addr: SocketAddr,
        packet: Vec<u8>,
        gamemode: u8,
        view_distance: i32,
        reply: Sender<PacketResult>,
    },
    // 网络层请求挖掉一格 挖完回一条 BlockBroken 事件
    BreakBlockFor {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
    },
    // 网络层请求放一格 放完回一条 BlockPlaced 事件
    PlaceBlockFor {
        addr: SocketAddr,
        // 被点击的方块 要存在才允许放置
        click: (i32, i32, i32),
        // 点击面的相邻格 目标位置
        target: (i32, i32, i32),
        id: u8,
    },
    // 方块被挖掉 属主分片就地生成掉落物
    AddDrop {
        eid: i64,
        item: crate::network::protocol::item::ItemStack,
        block: (i32, i32, i32),
    },
    // 新玩家出生 把本分片的掉落物补发给他
    SendDrops {
        addr: SocketAddr,
    },
    // 测试注入掉落物年龄
    #[cfg(test)]
    SetDropAge {
        index: usize,
        age: u32,
    },
    // 目前只有测试用来核对玩家归属 生产路径不查
    #[allow(dead_code)]
    PlayerCount {
        reply: Sender<usize>,
    },
    // IO 池把区块 load+decode 回来 插入缓存后再触发下发 不再阻塞 region 线程
    ChunkLoaded {
        x: i32,
        z: i32,
        chunk: Option<Chunk>,
    },
    // 提前让属主把跨界区块暖进缓存 玩家走到下发圈时直接走快路径
    // 不触发下发 只加载 回执时无条件插进缓存
    Prefetch {
        chunks: Vec<(i32, i32)>,
    },
    Stop,
}

// 一个区块加载完成之后该做什么 由发起方登记 回执时统一触发
enum Waiter {
    // 玩家自己的区块 加载完重新塞回下发队列
    Owned(SocketAddr),
    // 别人代发的跨界区块 加载完直接下发并结算
    Foreign { addr: SocketAddr },
}

// 属主分片处理 ForeignChunk 时 跟踪还有几个跨界区块在后台加载
struct ForeignPending {
    ack: RegionId,
    total: usize,
    remaining: usize,
}

// region 线程把区块加载移出主循环后 用这一组状态把任务交给 IO 池并对账
struct IoState {
    pool: Arc<ChunkIoPool>,
    // 已经交给 IO 池但还没回执的区块 防止同一个区块被重复提交
    loading: HashSet<(i32, i32)>,
    // 每个正在加载的区块上挂着的等待者 回执时逐个触发
    waiters: HashMap<(i32, i32), Vec<Waiter>>,
    // 进行中的跨界代发 归零才回 ForeignDone
    foreign_pending: HashMap<SocketAddr, ForeignPending>,
}

impl IoState {
    // 把区块交给 IO 池 已经在加载的就只挂等待者 不重复提交
    fn request(&mut self, x: i32, z: i32, waiter: Waiter, tx: &Sender<RegionMsg>) {
        if self.loading.insert((x, z)) {
            self.pool.submit(x, z, tx.clone());
        }
        self.waiters.entry((x, z)).or_default().push(waiter);
    }
}

pub struct RegionHandle {
    id: RegionId,
    tx: Sender<RegionMsg>,
    // region 线程产出的下行数据 主循环每 tick 取一次
    outbound: Arc<Mutex<Vec<RegionOutbound>>>,
    join: Option<JoinHandle<()>>,
}

impl RegionHandle {
    // loader 是跨 region 共享的 每个区域文件内部自己加锁
    // registry 登记自己 让别的分片能投递过来
    // io 是全局共享的区块加载线程池 region 线程不再同步读盘
    pub(crate) fn spawn(
        id: RegionId,
        loader: Option<Arc<ChunkLoader>>,
        generator: Generator,
        registry: Arc<RegionRegistry>,
        directory: Arc<PlayerDirectory>,
        idle_ticks: u64,
        view_distance: i32,
        io: Arc<ChunkIoPool>,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        registry.register(id, tx.clone());
        let bounds = id.chunk_bounds();
        let outbound = Arc::new(Mutex::new(Vec::new()));
        let thread_outbound = outbound.clone();
        let thread_registry = registry.clone();
        let thread_directory = directory.clone();
        let thread_tx = tx.clone();
        let name = format!("region-{}-{}", id.x, id.z);
        let join = thread::Builder::new()
            .name(name)
            .spawn(move || {
                let mut manager = match loader {
                    Some(loader) => ChunkManager::with_shared_loader(loader),
                    None => ChunkManager::new(),
                };
                manager.set_bounds(bounds);
                manager.set_generator(generator);
                run(
                    manager,
                    thread_tx,
                    rx,
                    id,
                    thread_outbound,
                    thread_registry,
                    thread_directory,
                    idle_ticks,
                    view_distance,
                    io,
                );
            })
            .expect("创建 region 线程失败");

        Self {
            id,
            tx,
            outbound,
            join: Some(join),
        }
    }

    // 玩家的区块下发视图交给本 region 队列从此由这里推进
    pub(crate) fn join(&self, addr: SocketAddr, player: RegionPlayer) {
        let _ = self.tx.send(RegionMsg::Join {
            addr,
            player: Box::new(player),
        });
    }

    pub(crate) fn leave(&self, addr: SocketAddr) -> bool {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::Leave { addr, reply }).is_err() {
            return false;
        }
        rx.recv().is_ok()
    }

    // 方块被挖掉 掉落物归本分片生成
    pub(crate) fn add_drop(
        &self,
        eid: i64,
        item: ItemStack,
        block: (i32, i32, i32),
    ) {
        let _ = self.tx.send(RegionMsg::AddDrop { eid, item, block });
    }

    // 新玩家出生 把本分片已有的掉落物补发给他
    pub(crate) fn send_drops(&self, addr: SocketAddr) {
        let _ = self.tx.send(RegionMsg::SendDrops { addr });
    }

    #[cfg(test)]
    pub(crate) fn set_drop_age(&self, index: usize, age: u32) {
        let _ = self.tx.send(RegionMsg::SetDropAge { index, age });
    }

    // 测试直接投递世界交互 生产路径由 region_handler 发消息
    #[cfg(test)]
    pub(crate) fn break_block_for(&self, addr: SocketAddr, x: i32, y: i32, z: i32) {
        let _ = self.tx.send(RegionMsg::BreakBlockFor { addr, x, y, z });
    }

    #[cfg(test)]
    pub(crate) fn place_block_for(
        &self,
        addr: SocketAddr,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        id: u8,
    ) {
        let _ = self
            .tx
            .send(RegionMsg::PlaceBlockFor { addr, click, target, id });
    }

    pub(crate) fn sync_player(&self, addr: SocketAddr, state: crate::entity::PlayerState) {
        let _ = self.tx.send(RegionMsg::SyncPlayer {
            addr,
            state: Box::new(state),
        });
    }

    // 投递本轮 GC 的可达区块
    pub(crate) fn gc(&self, live_chunks: HashSet<(i32, i32)>) {
        let _ = self.tx.send(RegionMsg::Gc { live_chunks });
    }

    // 包投递到属主分片并等它消费完 拿回下行与玩家状态快照
    pub(crate) fn handle_packet(
        &self,
        addr: SocketAddr,
        packet: &[u8],
        gamemode: u8,
        view_distance: i32,
    ) -> PacketResult {
        let (reply, rx) = mpsc::channel();
        let msg = RegionMsg::HandlePacket {
            addr,
            packet: packet.to_vec(),
            gamemode,
            view_distance,
            reply,
        };
        if self.tx.send(msg).is_err() {
            return PacketResult::empty(&HashMap::new());
        }
        rx.recv().unwrap_or_else(|_| PacketResult::empty(&HashMap::new()))
    }

    // 测试里核对玩家归属 生产路径不查
    #[allow(dead_code)]
    pub(crate) fn player_count(&self) -> usize {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::PlayerCount { reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    // 取走本 region 这一轮产出的下行数据
    pub fn drain_outbound(&self) -> Vec<RegionOutbound> {
        let mut buffer = self.outbound.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *buffer)
    }

    pub fn id(&self) -> RegionId {
        self.id
    }

    pub fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::GetBlock { x, y, z, reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    // 只读缓存 未命中返回 None 调用方(global tick)不会被同步读盘卡住
    pub fn payload(&self, chunk_x: i32, chunk_z: i32) -> Option<Bytes> {
        let (reply, rx) = mpsc::channel();
        let msg = RegionMsg::Payload {
            chunk_x,
            chunk_z,
            reply,
        };
        if self.tx.send(msg).is_err() {
            return None;
        }
        rx.recv().unwrap_or(None)
    }

    pub fn highest_block(&self, x: i32, z: i32) -> i32 {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::HighestBlock { x, z, reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    pub fn highest_blocks(&self, columns: &[(i32, i32)]) -> Vec<i32> {
        let (reply, rx) = mpsc::channel();
        let msg = RegionMsg::HighestBlocks {
            columns: columns.to_vec(),
            reply,
        };
        if self.tx.send(msg).is_err() {
            return vec![0; columns.len()];
        }
        rx.recv().unwrap_or_else(|_| vec![0; columns.len()])
    }

    // 本 region 已经推进的 tick 数
    pub fn tick_count(&self) -> u64 {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::TickCount { reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    pub fn flush(&self) -> usize {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::Flush { reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    // 异步请求落盘 不等回执 region 在后续 tick 里分批写完
    // 周期性保存走这条 调用方 global tick 不会被 region 的编码与写盘卡住
    pub fn flush_async(&self) {
        let _ = self.tx.send(RegionMsg::FlushAsync);
    }

    pub fn set_generator(&self, generator: Generator) {
        let (reply, rx) = mpsc::channel();
        if self
            .tx
            .send(RegionMsg::SetGenerator { generator, reply })
            .is_ok()
        {
            let _ = rx.recv();
        }
    }

    // 本 region 内存里现有的区块数
    pub fn len(&self) -> usize {
        let (reply, rx) = mpsc::channel();
        if self.tx.send(RegionMsg::Len { reply }).is_err() {
            return 0;
        }
        rx.recv().unwrap_or(0)
    }

    // 不做同步等待的投递 跨 region 通知用这个
    // 目前跨区还只走 join/leave 先留着接口
    #[allow(dead_code)]
    pub(crate) fn post(&self, msg: RegionMsg) -> bool {
        self.tx.send(msg).is_ok()
    }
}

impl Drop for RegionHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(RegionMsg::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

// 每个 region 自己的主循环 消息优先 tick 兜底
// 消息清空后才推进 tick 保证密集请求下 tick 不会被饿死
fn run(
    mut manager: ChunkManager,
    tx: Sender<RegionMsg>,
    rx: Receiver<RegionMsg>,
    id: RegionId,
    outbound: Arc<Mutex<Vec<RegionOutbound>>>,
    registry: Arc<RegionRegistry>,
    directory: Arc<PlayerDirectory>,
    idle_ticks: u64,
    view_distance: i32,
    io: Arc<ChunkIoPool>,
) {
    let mut io = IoState {
        pool: io,
        loading: HashSet::new(),
        waiters: HashMap::new(),
        foreign_pending: HashMap::new(),
    };
    let mut ticks: u64 = 0;
    // 连续空闲的 tick 数
    let mut idle_for: u64 = 0;
    // 收到异步落盘请求后 后续 tick 分批把脏块写完
    let mut flush_pending = false;
    // 最近一轮 GC 标记出来的可达区块 由 global region 统一算
    let mut gc_chunks: HashSet<(i32, i32)> = HashSet::new();
    let mut players: HashMap<SocketAddr, RegionPlayer> = HashMap::new();
    // 掉落物实体归本分片 物理与拾取判定都在本线程里做
    let mut drops: Vec<ItemDrop> = Vec::new();
    let mut next_tick = Instant::now() + REGION_TICK;

    loop {
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    if !dispatch(
                        &mut manager,
                        &mut players,
                        &mut drops,
                        msg,
                        id,
                        ticks,
                        &outbound,
                        &registry,
                        &directory,
                        &tx,
                        &mut gc_chunks,
                        view_distance,
                        &mut io,
                        &mut flush_pending,
                    ) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        let now = Instant::now();
        if now >= next_tick {
            ticks += 1;
            manager.tick(&gc_chunks);
            // 异步落盘请求到了就每 tick 分批写一点 平滑掉大批量改动的抖动
            if flush_pending {
                match manager.flush_limited(MAX_FLUSH_PER_TICK) {
                    Ok(0) => flush_pending = false,
                    Ok(_) => {}
                    Err(e) => {
                        warn!(region_x = id.x, region_z = id.z, error = %e, "区块落盘失败");
                        flush_pending = false;
                    }
                }
            }
            tick_players(&mut manager, &mut players, &outbound, &registry, id, &mut io, &tx);
            tick_drops(
                &mut manager,
                &mut drops,
                &directory,
                id,
                &outbound,
                view_distance,
            );

            // 没玩家也没实体就累计空闲 到点上报一次让 global region 撤线程
            // 线程自己不停 生命周期统一由 Level 管 免得留下悬空句柄
            if gc_chunks.is_empty() && players.is_empty() && drops.is_empty() {
                idle_for += 1;
                if idle_for >= idle_ticks {
                    idle_for = 0;
                    push_outbound(
                        &outbound,
                        RegionOutbound {
                            to: None,
                            skip: None,
                            around: None,
                            bodies: Vec::new(),
                            events: vec![RegionEvent::RegionIdle { region: id }],
                            chunks_complete: false,
                        },
                    );
                }
            } else {
                idle_for = 0;
            }
            next_tick += REGION_TICK;
            // 落后太多跳帧 不追赶 免得一帧里补跑十几个 tick
            if next_tick <= now {
                next_tick = now + REGION_TICK;
            }
        }

        let wait = next_tick.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(msg) => {
                if !dispatch(
                        &mut manager,
                        &mut players,
                        &mut drops,
                        msg,
                        id,
                        ticks,
                        &outbound,
                        &registry,
                        &directory,
                        &tx,
                        &mut gc_chunks,
                        view_distance,
                        &mut io,
                        &mut flush_pending,
                    ) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

// 本 region 内玩家的区块下发 生成与序列化都在 region 线程里做
// 视野跨到别的分片时 不去读别人的区块 改成把坐标投给属主 由属主直接下发给玩家
// 还没有加载进缓存的区块交给 IO 池后台读盘 不阻塞 region 线程
// 加载圈（Chebyshev 方形 比下发圈大一圈）提前交给 IO 池暖缓存
// 玩家走到下发圈照到时已在内存 走 cached 快路径 不再现场读盘
// 这一步绕过每 tick 的下发节流 让落盘区块并行加载 消除走进落盘区域的屏障
// 自己拥有的直接投 IO 别人拥有的发 Prefetch 让属主去暖
fn prefetch(
    manager: &ChunkManager,
    view: &crate::entity::player::PlayerView,
    io: &mut IoState,
    tx: &Sender<RegionMsg>,
    registry: &Arc<RegionRegistry>,
) {
    let (cx, cz) = view.chunk_center;
    let radius = view.radius;
    let mut foreign: HashMap<RegionId, Vec<(i32, i32)>> = HashMap::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            let x = cx + dx;
            let z = cz + dz;
            // 已在缓存或已在后台加载 跳过 避免重复投
            if manager.is_cached(x, z) || io.loading.contains(&(x, z)) {
                continue;
            }
            if manager.owns(x, z) {
                io.loading.insert((x, z));
                io.pool.submit(x, z, tx.clone());
            } else {
                foreign
                    .entry(RegionId::of_chunk(x, z))
                    .or_default()
                    .push((x, z));
            }
        }
    }
    // 跨界的一批发一条 Prefetch 给各自属主 属主自己去重是否要加载
    for (rid, chunks) in foreign {
        if let Some(target) = registry.lookup(rid) {
            let _ = target.send(RegionMsg::Prefetch { chunks });
        }
    }
}

fn tick_players(
    manager: &mut ChunkManager,
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    registry: &Arc<RegionRegistry>,
    id: RegionId,
    io: &mut IoState,
    tx: &Sender<RegionMsg>,
) {
    for (addr, player) in players.iter_mut() {
        let view = &mut player.view;
        // 加载圈提前暖缓存 走到下发圈时已在内存 不再现场读盘
        // 玩家没跨区块时加载圈不变 只在中心变化时才预取一次 省掉每 tick 的冗余消息
        if view.prefetch_center != Some(view.chunk_center) {
            prefetch(manager, view, io, tx, registry);
            view.prefetch_center = Some(view.chunk_center);
        }
        // 自己那份发完 别人代发的也收到回执 IO 也加载完 才算真的发完
        if view.pending_chunks.is_empty() && view.pending_foreign == 0 && view.pending_load == 0 {
            // 队列刚清空 通知一次 外层据此补出生状态
            if !view.completed {
                view.completed = true;
                player.state.spawned = true;
                player.state.spawn_pending = false;
                let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
                buffer.push(RegionOutbound {
                    to: Some(*addr),
                    skip: None,
                    around: None,
                    bodies: Vec::new(),
                    events: Vec::new(),
                    chunks_complete: true,
                });
            }
            continue;
        }
        // 只剩别人代发的那些 等回执到了再补完成通知
        if view.pending_chunks.is_empty() {
            continue;
        }

        let mut batch = Vec::with_capacity(CHUNKS_PER_TICK);
        for _ in 0..CHUNKS_PER_TICK {
            let Some(pos) = view.pending_chunks.pop_front() else {
                break;
            };
            batch.push(pos);
        }
        if batch.is_empty() {
            continue;
        }

        // 自己管的直接生成 别家管的按属主分组投过去
        // 还没在缓存里的本分片区块交给 IO 池 回来再重新排队下发
        let mut owned = Vec::with_capacity(batch.len());
        let mut foreign: HashMap<RegionId, Vec<(i32, i32)>> = HashMap::new();
        for pos in batch {
            if manager.owns(pos.0, pos.1) {
                if manager.is_cached(pos.0, pos.1) {
                    owned.push(pos);
                } else {
                    io.request(pos.0, pos.1, Waiter::Owned(*addr), tx);
                    view.pending_load += 1;
                }
            } else {
                foreign
                    .entry(RegionId::of_chunk(pos.0, pos.1))
                    .or_default()
                    .push(pos);
            }
        }

        for (owner, chunks) in foreign {
            let count = chunks.len();
            let delivered = match registry.lookup(owner) {
                Some(tx) => tx
                    .send(RegionMsg::ForeignChunk {
                        addr: *addr,
                        chunks: chunks.clone(),
                        ack: id,
                    })
                    .is_ok(),
                None => false,
            };
            if delivered {
                view.pending_foreign += count;
            } else {
                // 属主分片还没建起来 退回队列 下一轮再投
                warn!(
                    region_x = id.x,
                    region_z = id.z,
                    owner_x = owner.x,
                    owner_z = owner.z,
                    "跨界区块的属主分片不可用 已退回队列"
                );
                for pos in chunks {
                    view.pending_chunks.push_back(pos);
                }
            }
        }

        let mut bodies = Vec::with_capacity(owned.len());
        if !owned.is_empty() {
            let payloads = manager.payloads(&owned);
            for ((chunk_x, chunk_z), data) in owned.into_iter().zip(payloads) {
                bodies.push(
                    FullChunkDataPacket {
                        chunk_x,
                        chunk_z,
                        order: ORDER_COLUMNS,
                        data,
                    }
                    .encode(),
                );
            }
        }

        let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
        buffer.push(RegionOutbound {
            to: Some(*addr),
            skip: None,
            around: None,
            bodies,
            events: Vec::new(),
            chunks_complete: false,
        });
    }
}

// 掉落物的物理与拾取都在本分片线程里跑 方块查询直接走本地 ChunkManager
// 拾取判定读全局玩家目录快照 不跨 region 读别人的玩家状态
fn tick_drops(
    manager: &mut ChunkManager,
    drops: &mut Vec<ItemDrop>,
    directory: &Arc<PlayerDirectory>,
    id: RegionId,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    view_distance: i32,
) {
    let expired = {
        let mut probes = Vec::with_capacity(drops.len() * 2);
        for drop in drops.iter() {
            probes.extend_from_slice(&drop.probe_columns());
        }
        let blocks = manager.blocks(&probes);
        for (index, drop) in drops.iter_mut().enumerate() {
            drop.age += 1;
            drop.fall(blocks[index * 2], blocks[index * 2 + 1]);
        }
        let expired: Vec<(i64, (f32, f32, f32))> = drops
            .iter()
            .filter(|drop| drop.age > ITEM_LIFETIME)
            .map(|drop| (drop.eid, drop.position))
            .collect();
        drops.retain(|drop| drop.age <= ITEM_LIFETIME);
        expired
    };

    if !expired.is_empty() {
        let bodies = expired
            .iter()
            .map(|(eid, _)| RemoveEntityPacket { eid: *eid }.encode())
            .collect();
        // 过期消失也只给附近看得到这个掉落物的人
        let center = expired[0].1;
        push_outbound(
            outbound,
            RegionOutbound {
                to: None,
                skip: None,
                around: Some((center, view_radius_blocks(view_distance))),
                bodies,
                events: Vec::new(),
                chunks_complete: false,
            },
        );
    }

    // 找最近的拾取者 只按玩家目录快照算 先收集再改状态
    let players = directory.spawned();
    let mut picks: Vec<(usize, SocketAddr, i64)> = Vec::new();
    for (index, drop) in drops.iter().enumerate() {
        if drop.age < ITEM_PICKUP_DELAY {
            continue;
        }
        let nearest = players
            .iter()
            .filter(|(_, state)| drop.within_pickup_range(state.position))
            .map(|(addr, state)| {
                let dx = drop.position.0 - state.position.0;
                let dy = drop.position.1 - state.position.1;
                let dz = drop.position.2 - state.position.2;
                (*addr, state.eid, dx * dx + dy * dy + dz * dz)
            })
            .min_by(|(_, _, a), (_, _, b)| a.total_cmp(b));
        if let Some((addr, eid, _)) = nearest {
            picks.push((index, addr, eid));
        }
    }

    for (index, addr, collector_eid) in picks.into_iter().rev() {
        let (item_eid, item) = (drops[index].eid, drops[index].item);
        let center = drops[index].position;
        drops.remove(index);
        let remove = RemoveEntityPacket { eid: item_eid }.encode();
        let take_for_others = TakeItemEntityPacket {
            item_eid,
            collector_eid,
        }
        .encode();
        let take_for_self = TakeItemEntityPacket {
            item_eid,
            collector_eid: 0,
        }
        .encode();

        // 别人只看到"这个物品被拿走了" 限视野内
        push_outbound(
            outbound,
            RegionOutbound {
                to: None,
                skip: Some(addr),
                around: Some((center, view_radius_blocks(view_distance))),
                bodies: vec![take_for_others, remove.clone()],
                events: Vec::new(),
                chunks_complete: false,
            },
        );
        push_outbound(
            outbound,
            RegionOutbound {
                to: Some(addr),
                skip: None,
                around: None,
                bodies: vec![take_for_self, remove],
                events: Vec::new(),
                chunks_complete: false,
            },
        );
        // 背包刷新排在拾取包之后 单独一条事件让 global region 处理
        push_outbound(
            outbound,
            RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::Pickup { addr, item }],
                chunks_complete: false,
            },
        );
    }

    // 每 tick 同步一份快照 global region 按分片聚合出只读镜像
    push_outbound(
        outbound,
        RegionOutbound {
            to: None,
            skip: None,
            around: None,
            bodies: Vec::new(),
            events: vec![RegionEvent::DropsSnapshot {
                region: id,
                drops: drops.clone(),
            }],
            chunks_complete: false,
        },
    );
}

// 视距换算成可见半径 视野是按区块给的 广播按区块换算成格
fn view_radius_blocks(view_distance: i32) -> f32 {
    (view_distance * 16) as f32
}

fn push_outbound(buffer: &Arc<Mutex<Vec<RegionOutbound>>>, item: RegionOutbound) {
    let mut buffer = buffer.lock().unwrap_or_else(|e| e.into_inner());
    buffer.push(item);
}

// 返回 false 表示收到停机消息
fn dispatch(
    manager: &mut ChunkManager,
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    drops: &mut Vec<ItemDrop>,
    msg: RegionMsg,
    id: RegionId,
    ticks: u64,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    registry: &Arc<RegionRegistry>,
    directory: &Arc<PlayerDirectory>,
    tx: &Sender<RegionMsg>,
    gc_chunks: &mut HashSet<(i32, i32)>,
    view_distance: i32,
    io: &mut IoState,
    flush_pending: &mut bool,
) -> bool {
    match msg {
        RegionMsg::GetBlock { x, y, z, reply } => {
            let _ = reply.send(manager.block(x, y, z));
        }
        RegionMsg::Payload {
            chunk_x,
            chunk_z,
            reply,
        } => {
            let _ = reply.send(manager.payload_cached(chunk_x, chunk_z));
        }
        RegionMsg::HighestBlock { x, z, reply } => {
            let _ = reply.send(manager.highest_block(x, z));
        }
        RegionMsg::HighestBlocks { columns, reply } => {
            let _ = reply.send(manager.highest_blocks(&columns));
        }
        RegionMsg::Flush { reply } => match manager.flush() {
            Ok(n) => {
                let _ = reply.send(n);
            }
            Err(e) => {
                warn!(region_x = id.x, region_z = id.z, error = %e, "区块落盘失败");
                let _ = reply.send(0);
            }
        },
        // 异步落盘请求 标记一下 让 tick 里分批写完 不阻塞消息处理
        RegionMsg::FlushAsync => {
            *flush_pending = true;
        }
        RegionMsg::SetGenerator { generator, reply } => {
            manager.set_generator(generator);
            let _ = reply.send(());
        }
        RegionMsg::Len { reply } => {
            let _ = reply.send(manager.len());
        }
        RegionMsg::TickCount { reply } => {
            let _ = reply.send(ticks);
        }
        RegionMsg::Join { addr, player } => {
            players.insert(addr, *player);
        }
        RegionMsg::Leave { addr, reply } => {
            players.remove(&addr);
            let _ = reply.send(());
        }
        // 别的分片让我提前把跨界区块暖进缓存 不触发下发
        // 玩家走到下发圈照到时直接走 cached 快路径 不用现场读盘
        RegionMsg::Prefetch { chunks } => {
            for (x, z) in chunks {
                if manager.owns(x, z) && !manager.is_cached(x, z) && !io.loading.contains(&(x, z)) {
                    io.loading.insert((x, z));
                    io.pool.submit(x, z, tx.clone());
                }
            }
        }
        // 别的分片把跨界区块交给我 我负责生成/加载并直接下发给玩家
        // 缓存里没有的区块交给 IO 池后台读盘 加载完在 ChunkLoaded 里回执
        RegionMsg::ForeignChunk {
            addr,
            chunks,
            ack,
        } => {
            let mut pending = ForeignPending {
                ack,
                total: chunks.len(),
                remaining: chunks.len(),
            };
            for (chunk_x, chunk_z) in chunks {
                if manager.is_cached(chunk_x, chunk_z) {
                    let data = manager.payload(chunk_x, chunk_z);
                    let body = FullChunkDataPacket {
                        chunk_x,
                        chunk_z,
                        order: ORDER_COLUMNS,
                        data,
                    }
                    .encode();
                    let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
                    buffer.push(RegionOutbound {
                        to: Some(addr),
                        skip: None,
                        around: None,
                        bodies: vec![body],
                        events: Vec::new(),
                        chunks_complete: false,
                    });
                    pending.remaining -= 1;
                } else {
                    io.request(chunk_x, chunk_z, Waiter::Foreign { addr }, tx);
                }
            }
            if pending.remaining == 0 {
                if let Some(tx) = registry.lookup(ack) {
                    let _ = tx.send(RegionMsg::ForeignDone {
                        addr,
                        count: pending.total,
                    });
                }
            } else {
                io.foreign_pending.insert(addr, pending);
            }
        }
        // IO 池把区块 load+decode 回来 插进缓存后触发对应的等待者
        RegionMsg::ChunkLoaded { x, z, chunk } => {
            manager.insert_loaded_chunk(x, z, chunk);
            if let Some(waiters) = io.waiters.remove(&(x, z)) {
                for waiter in waiters {
                    match waiter {
                        Waiter::Owned(addr) => {
                            if let Some(player) = players.get_mut(&addr) {
                                player.view.pending_chunks.push_back((x, z));
                                player.view.pending_load =
                                    player.view.pending_load.saturating_sub(1);
                            }
                        }
                        Waiter::Foreign { addr } => {
                            let data = manager.payload(x, z);
                            let body = FullChunkDataPacket {
                                chunk_x: x,
                                chunk_z: z,
                                order: ORDER_COLUMNS,
                                data,
                            }
                            .encode();
                            let mut buffer =
                                outbound.lock().unwrap_or_else(|e| e.into_inner());
                            buffer.push(RegionOutbound {
                                to: Some(addr),
                                skip: None,
                                around: None,
                                bodies: vec![body],
                                events: Vec::new(),
                                chunks_complete: false,
                            });
                            if let Some(fp) = io.foreign_pending.get_mut(&addr) {
                                fp.remaining -= 1;
                                if fp.remaining == 0 {
                                    let (ack, total) = (fp.ack, fp.total);
                                    io.foreign_pending.remove(&addr);
                                    if let Some(t) = registry.lookup(ack) {
                                        let _ = t.send(RegionMsg::ForeignDone {
                                            addr,
                                            count: total,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            io.loading.remove(&(x, z));
        }
        // 别人代发的区块已经交付 结算掉等待中的计数
        RegionMsg::ForeignDone { addr, count } => {
            if let Some(player) = players.get_mut(&addr) {
                player.view.pending_foreign = player.view.pending_foreign.saturating_sub(count);
            }
        }
        // 网络层请求挖方块 挖完把结果作为事件交回去
        RegionMsg::BreakBlockFor { addr, x, y, z } => {
            let id = manager.break_block(x, y, z);
            let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
            buffer.push(RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::BlockBroken { addr, x, y, z, id }],
                chunks_complete: false,
            });
        }
        // 网络层请求放方块 点击的方块要在 目标格要空
        RegionMsg::PlaceBlockFor {
            addr,
            click,
            target,
            id,
        } => {
            let (cx, cy, cz) = click;
            let (tx, ty, tz) = target;
            let ok = manager.block(cx, cy, cz) != 0
                && manager.block(tx, ty, tz) == 0
                && manager.set_block(tx, ty, tz, id);
            let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
            buffer.push(RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::BlockPlaced {
                    addr,
                    x: tx,
                    y: ty,
                    z: tz,
                    id,
                    ok,
                }],
                chunks_complete: false,
            });
        }
        // 方块被挖掉 掉落物由属主分片自己生成并广播
        RegionMsg::AddDrop { eid, item, block } => {
            let drop = ItemDrop::new(eid, item, block);
            let bodies = vec![drop.add_packet()];
            let center = drop.position;
            drops.push(drop);
            push_outbound(outbound, RegionOutbound {
                to: None,
                skip: None,
                around: Some((center, view_radius_blocks(view_distance))),
                bodies,
                events: Vec::new(),
                chunks_complete: false,
            });
        }
        // 新玩家出生 把本分片已有的掉落物补发给他
        RegionMsg::SendDrops { addr } => {
            if !drops.is_empty() {
                let bodies: Vec<Bytes> = drops.iter().map(|drop| drop.add_packet()).collect();
                push_outbound(outbound, RegionOutbound {
                    to: Some(addr),
                    skip: None,
                    around: None,
                    bodies,
                    events: Vec::new(),
                    chunks_complete: false,
                });
            }
        }
        #[cfg(test)]
        RegionMsg::SetDropAge { index, age } => {
            if let Some(drop) = drops.get_mut(index) {
                drop.age = age;
            }
        }
        RegionMsg::Gc { live_chunks } => {
            *gc_chunks = live_chunks;
        }
        RegionMsg::Transfer {
            addr,
            player,
            view_distance,
        } => {
            // 视野按新位置重建 区块队列重新排队
            players.insert(addr, RegionPlayer::new(player.state, view_distance));
        }
        // 包在属主分片里消费 状态与广播都在这里落地
        RegionMsg::HandlePacket {
            addr,
            packet,
            gamemode,
            view_distance,
            reply,
        } => {
            let result = region_handler::handle_packet(
                players,
                directory,
                registry,
                tx,
                id,
                addr,
                &packet,
                gamemode,
                view_distance,
            );
            let events = result.events;
            let result = region_handler::PacketResult {
                commands: result.commands,
                players: result.players,
                events: Vec::new(),
            };
            if !events.is_empty() {
                push_outbound(
                    outbound,
                    RegionOutbound {
                        to: None,
                        skip: None,
                        around: None,
                        bodies: Vec::new(),
                        events,
                        chunks_complete: false,
                    },
                );
            }
            let _ = reply.send(result);
        }
        RegionMsg::SyncPlayer { addr, state } => {
            if let Some(player) = players.get_mut(&addr) {
                player.state = *state;
            }
        }
        RegionMsg::PlayerCount { reply } => {
            let _ = reply.send(players.len());
        }
        RegionMsg::Stop => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::PlayerState;
    use crate::level::generator::flat::{BEDROCK, GRASS};
    use crate::level::generator::Generator;

    fn spawn_flat(x: i32, z: i32) -> RegionHandle {
        spawn_flat_with(
            x,
            z,
            Arc::new(RegionRegistry::new()),
            Arc::new(PlayerDirectory::new()),
        )
    }

    // 多个分片共用一个注册表与玩家目录 就能互相投递
    fn spawn_flat_with(
        x: i32,
        z: i32,
        registry: Arc<RegionRegistry>,
        directory: Arc<PlayerDirectory>,
    ) -> RegionHandle {
        RegionHandle::spawn(
            RegionId { x, z },
            None,
            Generator::flat(),
            registry,
            directory,
            IDLE_SHUTDOWN_TICKS,
            crate::config::DEFAULT_VIEW_DISTANCE,
            ChunkIoPool::new(None, 1),
        )
    }

    // 写方块要走属主分片 挖完以事件回执 拿回原来的 id
    fn break_and_wait(region: &RegionHandle, x: i32, y: i32, z: i32) -> Option<u8> {
        let addr: std::net::SocketAddr = "127.0.0.1:50099".parse().unwrap();
        region.break_block_for(addr, x, y, z);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockBroken { id, .. } = event {
                        return id;
                    }
                }
            }
            assert!(Instant::now() < deadline, "挖掘没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // 放方块也要走属主分片 放完以事件回执
    fn place_and_wait(
        region: &RegionHandle,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        id: u8,
    ) -> bool {
        let addr: std::net::SocketAddr = "127.0.0.1:50098".parse().unwrap();
        region.place_block_for(addr, click, target, id);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockPlaced { ok, .. } = event {
                        return ok;
                    }
                }
            }
            assert!(Instant::now() < deadline, "放置没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn player_at(position: (f32, f32, f32), radius: i32) -> RegionPlayer {
        let mut state = PlayerState::default();
        state.position = position;
        RegionPlayer::new(state, radius)
    }

    #[test]
    fn reads_and_writes_through_the_thread() {        let region = spawn_flat(0, 0);
        assert_eq!(region.block(0, 0, 0), BEDROCK);
        assert_eq!(region.block(0, 4, 0), GRASS);

        assert_eq!(break_and_wait(&region, 0, 4, 0), Some(GRASS));
        assert_eq!(region.block(0, 4, 0), 0);
        // 已经是空气 再挖一次没有东西
        assert_eq!(break_and_wait(&region, 0, 4, 0), None);
    }

    #[test]
    fn payload_reflects_edits() {
        let region = spawn_flat(0, 0);
        // 先触发生成让区块进缓存 payload 现在只读缓存不触发读盘
        assert_eq!(region.block(0, 4, 0), GRASS);
        assert_eq!(region.payload(0, 0).expect("区块已生成")[5], 0);
        // 点草方块的上表面 往上面放一块石头
        assert!(place_and_wait(&region, (0, 4, 0), (0, 5, 0), 3));
        assert_eq!(region.payload(0, 0).expect("区块已生成")[5], 3);
    }

    // 两个 region 各自独立 状态不串
    #[test]
    fn regions_do_not_share_state() {
        let a = spawn_flat(0, 0);
        let b = spawn_flat(1, 0);

        break_and_wait(&a, 0, 5, 0);
        assert_eq!(a.block(0, 5, 0), 0);
        assert_eq!(b.block(0, 5, 0), 0);
    }

    // 没有存档时落盘是空操作 但消息必须能正常往返
    #[test]
    fn flush_without_storage_is_noop() {
        let region = spawn_flat(0, 0);
        break_and_wait(&region, 0, 5, 0);
        assert_eq!(region.flush(), 0);
    }

    // 跨线程投递的延迟 决定调用方应该搬进 region 还是留在外面
    #[test]
    #[ignore]
    fn probe_round_trip_latency() {
        let region = spawn_flat(0, 0);
        region.block(0, 4, 0);

        let rounds = 100_000u32;
        let t = std::time::Instant::now();
        let mut sum = 0u32;
        for _ in 0..rounds {
            sum += region.block(0, 4, 0) as u32;
        }
        let elapsed = t.elapsed();
        println!(
            "block round-trip: {:.3} us/op (sum={sum})",
            elapsed.as_secs_f64() * 1e6 / rounds as f64
        );
    }

    // region 自己按周期推进 tick 不需要外部驱动
    #[test]
    fn ticks_on_its_own() {
        let region = spawn_flat(0, 0);
        std::thread::sleep(Duration::from_millis(130));
        let ticks = region.tick_count();
        assert!(ticks >= 2, "只跑了 {ticks} 个 tick");
    }

    // 密集请求下消息优先 tick 不会被饿死
    #[test]
    fn requests_are_not_starved_by_tick() {
        let region = spawn_flat(0, 0);
        for _ in 0..200 {
            assert_eq!(region.block(0, 0, 0), BEDROCK);
        }
    }

    // 玩家归属 region 之后 区块由 region 自己的 tick 生成并推出来
    #[test]
    fn player_chunks_are_produced_by_region_tick() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50000".parse().unwrap();

        // 站在分片中间 半径 1 的下发圈是完整 3x3 九块 都归自己
        region.join(addr, player_at((100.5, 5.0, 100.5), 1));

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut bodies = 0usize;
        loop {
            let out = region.drain_outbound();
            for item in &out {
                if item.to.is_some() {
                    assert_eq!(item.to, Some(addr));
                }
                bodies += item.bodies.len();
            }
            if out.iter().any(|item| item.chunks_complete) {
                break;
            }
            assert!(Instant::now() < deadline, "区块没有按时发完");
            std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(bodies, 9);
            }

    // 视野跨到别的分片时 那些区块由属主分片生成并直接下发
    // 本分片不读别人的区块 也不经过主线程
    #[test]
    fn border_chunks_are_delivered_by_owning_region() {
        let registry = Arc::new(RegionRegistry::new());
        let directory = Arc::new(PlayerDirectory::new());
        let home = spawn_flat_with(0, 0, registry.clone(), directory.clone());
        let west = spawn_flat_with(-1, 0, registry.clone(), directory.clone());
        let north = spawn_flat_with(0, -1, registry.clone(), directory.clone());
        let corner = spawn_flat_with(-1, -1, registry.clone(), directory.clone());
        let addr: std::net::SocketAddr = "127.0.0.1:50003".parse().unwrap();

        // 玩家站在 (0,0) 区块 半径 1 的 3x3 里有五块属于别的分片
        home.join(addr, player_at((0.5, 5.0, 0.5), 1));

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut count = 0usize;
        loop {
            let mut done = false;
            for region in [&home, &west, &north, &corner] {
                for item in region.drain_outbound() {
                    if item.to.is_some() {
                        assert_eq!(item.to, Some(addr));
                    }
                    count += item.bodies.len();
                    done |= item.chunks_complete;
                }
            }
            // 全部交付完由玩家所在分片报完成
            if done {
                break;
            }
            assert!(Instant::now() < deadline, "跨界区块没有按时发完");
            std::thread::sleep(Duration::from_millis(20));
        }
        // 四块归自己 五块归隔壁 加起来是完整的九块
        assert_eq!(count, 9);
    }

    // 队列清空后 region 主动通知一次 外层据此补出生状态
    #[test]
    fn region_reports_when_chunks_are_done() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50002".parse().unwrap();
        region.join(addr, player_at((0.5, 5.0, 0.5), 0));

        // 先发区块再报完成 至少两个 tick 周期 这里轮询避免卡死时限
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let out = region.drain_outbound();
            if out.iter().any(|item| item.chunks_complete) {
                break;
            }
            assert!(Instant::now() < deadline, "没有收到队列清空通知");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // 玩家离开后 region 不再保留他
    #[test]
    fn leaving_player_clears_state() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50001".parse().unwrap();

        region.join(addr, player_at((0.5, 5.0, 0.5), 1));
        std::thread::sleep(Duration::from_millis(30));

        assert!(region.leave(addr));
        assert_eq!(region.player_count(), 0);
    }

    // 句柄释放时线程要停下来 不能泄漏
    #[test]
    fn drop_stops_the_thread() {
        let region = spawn_flat(0, 0);
        let id = region.id();
        assert_eq!(id, RegionId { x: 0, z: 0 });
        drop(region);
    }
}
