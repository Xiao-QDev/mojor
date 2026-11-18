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

use crate::entity::{FallingBlock, ItemDrop, PlayerDirectory, RegionPlayer, within_view_distance};
use crate::entity::{ITEM_LIFETIME, ITEM_PICKUP_DELAY};
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::item_entity::TakeItemEntityPacket;
use crate::network::protocol::move_entity::{MoveEntityEntry, MoveEntityPacket};
use crate::network::protocol::move_player::{EYE_HEIGHT, MovePlayerPacket};
use crate::network::protocol::remove_entity::RemoveEntityPacket;
use crate::network::protocol::update_block::UpdateBlockPacket;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::chunk_io::ChunkIoPool;
use crate::block::placement::Placement;
use crate::level::chunk_manager::ChunkManager;
use crate::level::format::chunk::Chunk;
use crate::level::physics::{self, CellChange, UpdateQueue};
use crate::level::light::{LightReceipt, LightService};
use crate::level::generator::Generator;
use crate::level::region::RegionId;
use crate::level::region_handler::{self, PacketResult};
use crate::network::protocol::full_chunk_data::{FullChunkDataPacket, ORDER_COLUMNS};

// 每个 region 自己的 tick 周期 与主循环一致
pub const REGION_TICK: Duration = Duration::from_millis(50);
// 同一个玩家两次挖掘之间的最小间隔 2 tick = 100ms
// 生存里最快的合法挖掘(带工具瞬挖)也就这么快 再快的先排队
pub const MIN_DIG_TICKS: u64 = 2;
// 排队上限 超过说明流量远超任何合法速度 才回退成拒绝(客户端会看到回弹)
const MAX_DIG_QUEUE: usize = 4;
// 每个玩家每 tick 最多下发多少个区块 与网络层保持一致
const CHUNKS_PER_TICK: usize = 4;
// 连续这么多 tick 不可达就上报空闲 40 tick 也就是 2 秒
pub const IDLE_SHUTDOWN_TICKS: u64 = 40;
// 每 tick 最多编码写出多少脏块 16 块约 1.3MB 分摊进 50ms 不会被察觉
const MAX_FLUSH_PER_TICK: usize = 16;
// 停机 drain 的兜底上限：光照 worker 被 CPU 饿住时回执可能很慢，
// 但分片不能在没落盘的情况下直接退线程，宁可多等也别丢编辑
const STOP_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

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
    // 挖一格的回执 id/meta 是这一格原本的样子(成功时就是被挖掉的那个方块)
    BlockBroken {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        // 挖掉前的 meta 碎屑粒子靠它选贴图
        meta: u8,
        // false 表示没挖动(基岩/本来就是空气) 要让客户端把这一格补回去
        ok: bool,
        // 门挖一半时连带的另一半 (x,y,z,id,meta)
        paired: Option<(i32, i32, i32, u8, u8)>,
    },
    // 放置结果 门/床 一次两格 ok 为 false 表示目标格被占 没有真的放下去
    BlockPlaced {
        addr: SocketAddr,
        // 第一格(被点面相邻格)的绝对坐标 其余格按 cells 里的偏移推
        origin: (i32, i32, i32),
        cells: Placement,
        // 手持物品 id 失败时要按物品退回去 不是方块 id
        item: i16,
        ok: bool,
    },
    // 服务端自发的方块改动(液体流动) 只让 global 广播 不碰背包
    BlockChanged {
        // (x, y, z, 新 id, 新 meta) 每格一条 UpdateBlockPacket
        cells: Vec<CellChange>,
    },
    // 重力方块失去支撑 请求 global 分配 eid 后回投 AddFallingBlock 生成实体
    // 此时方块还在原地 由回投消息负责清除并生成实体
    FallRequested {
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        meta: u8,
    },
    // 下落方块落地时目标格被占 原版 spawnAtLocation 掉落物品
    // region 线程没有 eid 分配器 交给 global 分配后回投 AddDrop
    FallingBlockDropped {
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        meta: u8,
        creative: bool,
    },
    // 玩家投掷/发射投射物 region 线程没有 eid 分配器
    // 交给 global 分配后回投 LaunchProjectile 让属主分片生成实体
    LaunchProjectile {
        kind: crate::entity::projectile::ProjectileKind,
        position: (f32, f32, f32),
        direction: (f32, f32, f32),
        speed: f32,
    },
    // 下落方块砸中玩家 让 global 扣血并下发 SetHealth/受击动画
    DamagePlayer {
        addr: SocketAddr,
        damage: i32,
    },
    // 点门开关的结果 翻转的是下半格 meta 的 0x04
    // 只有真的翻了门才会发这条 非门/没配对下半一律静默
    DoorToggled {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        meta: u8,
    },
    // 客户端拖动某个容器窗口的槽位 由 global 的容器注册表存档
    ContainerSlot {
        addr: SocketAddr,
        window_id: u8,
        slot: usize,
        item: Option<crate::network::protocol::item::ItemStack>,
    },
    // 关窗 清掉玩家与容器的关联
    ContainerClosed { addr: SocketAddr },
    // 点了工作台/熔炉/箱子 让 global 下发 ContainerOpen 打开界面
    ContainerOpen {
        addr: SocketAddr,
        window_id: u8,
        kind: u8,
        slots: u16,
        x: i32,
        y: i32,
        z: i32,
        // 熔炉朝向 点亮/熄火广播方块时要保留
        meta: u8,
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
    // 分片每秒上报一次自身耗时与规模 global region 只读镜像供 /tps 展示
    Health {
        health: crate::tick::metrics::RegionHealth,
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
    // 测试读方块元数据(羊毛色 楼梯朝向)
    #[cfg(test)]
    GetBlockMeta {
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
    // global region 发起的传送 权威坐标与区块队列都在属主分片里改
    Teleport {
        addr: SocketAddr,
        position: (f32, f32, f32),
        yaw: f32,
        pitch: f32,
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
    // 熔炉点燃/熄火 分片把方块 id 在 61/62 之间切 客户端重载区块也一致
    SetFurnaceLit {
        x: i32,
        y: i32,
        z: i32,
        lit: bool,
    },
    // 网络层请求开关门 分片翻转下半格 meta 的 0x04 回一条 DoorToggled
    ToggleDoorFor {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
    },
    // 网络层请求打开被点方块的容器 UI 分片读方块判定窗口类型
    OpenContainerFor {
        addr: SocketAddr,
        x: i32,
        y: i32,
        z: i32,
    },
    // 网络层请求放置 放完回一条 BlockPlaced 事件
    PlaceBlockFor {
        addr: SocketAddr,
        // 被点击的方块 要存在才允许放置
        click: (i32, i32, i32),
        // 点击面的相邻格 第一格的绝对位置
        target: (i32, i32, i32),
        // 对照表算好的结果 相对 target 的偏移
        cells: Placement,
        // 手持物品 id 放置失败要原样退回
        item: i16,
    },
    // 发射投射物(弓箭/雪球/蛋/末影珍珠) global 分配 eid 后回投属主分片生成
    LaunchProjectile {
        eid: i64,
        kind: crate::entity::projectile::ProjectileKind,
        position: (f32, f32, f32),
        direction: (f32, f32, f32),
        speed: f32,
    },
    // 功能物品(桶/打火石/锄头/骨粉)对方块使用
    // 分片读世界方块后用纯函数判定 原子应用改动 + 回写背包物品
    UseItemOn {
        addr: SocketAddr,
        item: crate::network::protocol::item::ItemStack,
        // 被点方块坐标 和 点击面相邻格(放置位置)
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        slot: usize,
    },
    // 方块被挖掉 属主分片就地生成掉落物
    AddDrop {
        eid: i64,
        item: crate::network::protocol::item::ItemStack,
        block: (i32, i32, i32),
    },
    // global 分配好 eid 后回投: 清除重力方块并生成下落实体
    AddFallingBlock {
        eid: i64,
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        meta: u8,
        creative: bool,
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
    // 测试直接写方块(绕过放置判定) 用于布置物理场景
    #[cfg(test)]
    SetBlockForTest {
        x: i32,
        y: i32,
        z: i32,
        id: u8,
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
    // 光照 worker 把算好的光照发回来 写回缓存后让等待的玩家重新排队
    LightDone {
        receipt: LightReceipt,
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
    // 等光照算完才能下发的跨界区块：坐标 -> (目标玩家, 请求方分片)。
    // 属主分片里刚生成的区块 lit=false、天空光全 0，此时若按 ForeignChunk 直接
    // payload() 下发，客户端拿到的是全黑区块——能挖有碰撞但画面空白。
    // 这里先挂起，等 LightDone 把光照回写、lit 打开后再真正下发并结算。
    foreign_lit_wait: HashMap<(i32, i32), Vec<(SocketAddr, RegionId)>>,
}

impl IoState {
    // 把区块交给 IO 池 已经在加载的就只挂等待者 不重复提交
    fn request(&mut self, x: i32, z: i32, waiter: Waiter, tx: &Sender<RegionMsg>) {
        if self.loading.insert((x, z)) {
            self.pool.submit(x, z, tx.clone());
        }
        self.waiters.entry((x, z)).or_default().push(waiter);
    }

    // 归属一个跨界代发的 remaining，归零时回 ForeignDone 让请求方结算 pending_foreign
    // 返回本块是否为该玩家的最后一块
    fn settle_foreign(&mut self, addr: SocketAddr, registry: &Arc<RegionRegistry>) {
        let done = if let Some(fp) = self.foreign_pending.get_mut(&addr) {
            fp.remaining = fp.remaining.saturating_sub(1);
            fp.remaining == 0
        } else {
            false
        };
        if done {
            if let Some(fp) = self.foreign_pending.remove(&addr) {
                if let Some(t) = registry.lookup(fp.ack) {
                    let _ = t.send(RegionMsg::ForeignDone {
                        addr,
                        count: fp.total,
                    });
                }
            }
        }
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
        light: Arc<LightService>,
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
                    light,
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

    // global 分配好 eid 后让属主分片生成下落实体
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn add_falling_block(
        &self,
        eid: i64,
        x: i32,
        y: i32,
        z: i32,
        id: u8,
        meta: u8,
        creative: bool,
    ) {
        let _ = self.tx.send(RegionMsg::AddFallingBlock {
            eid, x, y, z, id, meta, creative,
        });
    }

    // global 分配好 eid 后让抛出点分片生成投射物实体
    pub(crate) fn launch_projectile(
        &self,
        eid: i64,
        kind: crate::entity::projectile::ProjectileKind,
        position: (f32, f32, f32),
        direction: (f32, f32, f32),
        speed: f32,
    ) {
        let _ = self.tx.send(RegionMsg::LaunchProjectile {
            eid, kind, position, direction, speed,
        });
    }

    pub(crate) fn set_furnace_lit(&self, x: i32, y: i32, z: i32, lit: bool) {
        let _ = self.tx.send(RegionMsg::SetFurnaceLit { x, y, z, lit });
    }

    // 新玩家出生 把本分片已有的掉落物补发给他
    pub(crate) fn send_drops(&self, addr: SocketAddr) {
        let _ = self.tx.send(RegionMsg::SendDrops { addr });
    }

    #[cfg(test)]
    pub(crate) fn set_drop_age(&self, index: usize, age: u32) {
        let _ = self.tx.send(RegionMsg::SetDropAge { index, age });
    }

    #[cfg(test)]
    pub(crate) fn set_block_for_test(&self, x: i32, y: i32, z: i32, id: u8) {
        let _ = self.tx.send(RegionMsg::SetBlockForTest { x, y, z, id });
    }

    // 测试直接投递世界交互 生产路径由 region_handler 发消息
    #[cfg(test)]
    pub(crate) fn break_block_for(&self, addr: SocketAddr, x: i32, y: i32, z: i32) {
        let _ = self.tx.send(RegionMsg::BreakBlockFor { addr, x, y, z });
    }

    #[cfg(test)]
    pub(crate) fn toggle_door_for(&self, addr: SocketAddr, x: i32, y: i32, z: i32) {
        let _ = self.tx.send(RegionMsg::ToggleDoorFor { addr, x, y, z });
    }

    #[cfg(test)]
    pub(crate) fn place_block_for(
        &self,
        addr: SocketAddr,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        id: u8,
    ) {
        let _ = self.tx.send(RegionMsg::PlaceBlockFor {
            addr,
            click,
            target,
            cells: Placement::single(id, 0),
            item: id as i16,
        });
    }

    // 门/床 这种一次两格的放置 测试直接投递对照表算好的结果
    #[cfg(test)]
    pub(crate) fn place_cells_for(
        &self,
        addr: SocketAddr,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        cells: Placement,
    ) {
        let _ = self.tx.send(RegionMsg::PlaceBlockFor {
            addr,
            click,
            target,
            cells,
            item: cells.first.id as i16,
        });
    }

    pub(crate) fn sync_player(&self, addr: SocketAddr, state: crate::entity::PlayerState) {
        let _ = self.tx.send(RegionMsg::SyncPlayer {
            addr,
            state: Box::new(state),
        });
    }

    // 服务端主动传送 位置权威在分片 这里只投递 不等回执
    pub(crate) fn teleport(
        &self,
        addr: SocketAddr,
        position: (f32, f32, f32),
        yaw: f32,
        pitch: f32,
    ) {
        let _ = self.tx.send(RegionMsg::Teleport {
            addr,
            position,
            yaw,
            pitch,
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

    // 测试读元数据
    #[cfg(test)]
    pub(crate) fn meta(&self, x: i32, y: i32, z: i32) -> u8 {
        let (reply, rx) = mpsc::channel();
        let msg = RegionMsg::GetBlockMeta { x, y, z, reply };
        if self.tx.send(msg).is_err() {
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
    light: Arc<LightService>,
) {
    let mut io = IoState {
        pool: io,
        loading: HashSet::new(),
        waiters: HashMap::new(),
        foreign_pending: HashMap::new(),
        foreign_lit_wait: HashMap::new(),
    };
    let mut ticks: u64 = 0;
    // 方块物理更新队列 液体/重力方块按到期 tick 逐格处理 只含本分片坐标
    let mut physics_queue: UpdateQueue = UpdateQueue::default();
    // 连续空闲的 tick 数
    let mut idle_for: u64 = 0;
    // 已提交给光照引擎、还没收到回执的任务数
    // 等价于"这段时间里内存里可能有还没照亮也没落盘的改动"
    let mut light_in_flight: usize = 0;
    // 收到异步落盘请求后 后续 tick 分批把脏块写完
    let mut flush_pending = false;
    // 最近一轮 GC 标记出来的可达区块 由 global region 统一算
    let mut gc_chunks: HashSet<(i32, i32)> = HashSet::new();
    let mut players: HashMap<SocketAddr, RegionPlayer> = HashMap::new();
    // 掉落物实体归本分片 物理与拾取判定都在本线程里做
    let mut drops: Vec<ItemDrop> = Vec::new();
    // 下落方块实体(沙子/砂砾/铁砧) 失去支撑时由 global 分配 eid 后在此生成
    let mut falling_blocks: Vec<FallingBlock> = Vec::new();
    // 投射物实体(箭/雪球/蛋/末影珍珠) 由玩家使用物品触发
    let mut projectiles: Vec<crate::entity::projectile::Projectile> = Vec::new();
    // 已请求下落但还没回投生成实体的格子 避免重复请求
    let mut falls_pending: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut next_tick = Instant::now() + REGION_TICK;
    // 分片健康上报窗口 20 拍(1 秒)一次
    const HEALTH_TICKS: u64 = 20;
    let mut health_mspt_sum = 0.0_f64;
    let mut health_max_mspt = 0.0_f32;
    let mut health_ticks = 0_u64;
    let mut health_start = Instant::now();

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
                        &mut light_in_flight,
                        &mut physics_queue,
                        &mut falling_blocks,
                        &mut falls_pending,
                        &mut projectiles,
                    ) {
                        stop_drain(&mut manager, &light, &tx, &rx, &mut light_in_flight);
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // 通道没了收不到回执，能写的先写下去
                    let _ = manager.flush();
                    return;
                }
            }
        }

        let now = Instant::now();
        if now >= next_tick {
            ticks += 1;
            // 本拍开工时间 MSPT 统计到这个 tick 分支末尾
            let tick_start = Instant::now();
            manager.tick(&gc_chunks);
            // 方块物理: 处理到期的液体流动 本线程独占读写 结果交给 global 广播
            // 重力方块单独走 fall_check -> FallRequested -> global 分配 eid -> AddFallingBlock
            let due = physics_queue.take_due(ticks);
            if !due.is_empty() {
                let mut changed: Vec<CellChange> = Vec::new();
                let mut fall_events: Vec<RegionEvent> = Vec::new();
                for (x, y, z) in due {
                    // 失去支撑的重力方块: 请求生成下落实体(去重) 不在这里改方块
                    if let Some((bid, meta)) = physics::fall_check(&mut manager, x, y, z) {
                        if falls_pending.insert((x, y, z)) {
                            fall_events.push(RegionEvent::FallRequested { x, y, z, id: bid, meta });
                        }
                    } else {
                        changed.extend(physics::step(&mut manager, &mut physics_queue, x, y, z, ticks));
                    }
                }
                if !changed.is_empty() {
                    push_outbound(
                        &outbound,
                        RegionOutbound {
                            to: None,
                            skip: None,
                            around: None,
                            bodies: Vec::new(),
                            events: vec![RegionEvent::BlockChanged { cells: changed }],
                            chunks_complete: false,
                        },
                    );
                }
                if !fall_events.is_empty() {
                    push_outbound(
                        &outbound,
                        RegionOutbound {
                            to: None,
                            skip: None,
                            around: None,
                            bodies: Vec::new(),
                            events: fall_events,
                            chunks_complete: false,
                        },
                    );
                }
            }
            // 攒下的光照任务交给异步引擎 快照已在 region 线程抓好
            let jobs = manager.take_light_jobs(&tx);
            light_in_flight += jobs.len();
            for job in jobs {
                light.submit(job);
            }
            // 异步落盘请求到了就每 tick 分批写一点 平滑掉大批量改动的抖动
            if flush_pending {
                match manager.flush_limited(MAX_FLUSH_PER_TICK) {
                    Ok(0) => flush_pending = false,
                    Ok(_) => {}
                    Err(_) => {
                        flush_pending = false;
                    }
                }
            }
            // 饥饿系统: 冷却递减 + 回血/饿死 + 疲劳衰减
            tick_hunger(&mut players, &outbound, ticks);
            tick_players(&mut manager, &mut players, &outbound, &registry, id, &mut io, &tx);
            // 上一拍排队没来得及挖的 按节流节奏补上
            drain_digs(&mut manager, &mut physics_queue, &mut players, &outbound, ticks);
            tick_drops(
                &mut manager,
                &mut drops,
                &directory,
                id,
                &outbound,
                view_distance,
            );
            tick_falling_blocks(
                &mut manager,
                &mut physics_queue,
                &mut falling_blocks,
                &directory,
                id,
                &outbound,
                view_distance,
                ticks,
            );
            tick_projectiles(&mut manager, &mut projectiles, &outbound, view_distance);

            // 没玩家也没实体就累计空闲 到点上报一次让 global region 撤线程
            // 线程自己不停 生命周期统一由 Level 管 免得留下悬空句柄
            // 光照有在途/待提交的也不算空闲：这时候退线程，未 lit 的区块
            // 会被 flush 跳过，玩家的编辑就跟着内存一起没了
            if gc_chunks.is_empty()
                && players.is_empty()
                && drops.is_empty()
                && falling_blocks.is_empty()
                && projectiles.is_empty()
                && falls_pending.is_empty()
                && light_in_flight == 0
                && !manager.has_pending_light()
                && physics_queue.is_empty()
            {
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

            // 秒级窗口统计 攒够 20 拍推给 global region 供 /tps 展示
            let tick_mspt = tick_start.elapsed().as_secs_f64() * 1000.0;
            health_mspt_sum += tick_mspt;
            health_max_mspt = health_max_mspt.max(tick_mspt as f32);
            health_ticks += 1;
            if health_ticks >= HEALTH_TICKS {
                let elapsed = health_start.elapsed().as_secs_f32().max(f32::MIN_POSITIVE);
                let health = crate::tick::metrics::RegionHealth {
                    region: (id.x, id.z),
                    mspt: (health_mspt_sum / health_ticks as f64) as f32,
                    max_mspt: health_max_mspt,
                    // 实际完成拍数除以真实时长 掉拍时低于标称 20
                    tps: (health_ticks as f32 / elapsed).min(crate::tick::metrics::TARGET_TPS as f32),
                    chunks: manager.loaded_chunks(),
                    players: players.len(),
                    entities: drops.len(),
                };
                push_outbound(
                    &outbound,
                    RegionOutbound {
                        to: None,
                        skip: None,
                        around: None,
                        bodies: Vec::new(),
                        events: vec![RegionEvent::Health { health }],
                        chunks_complete: false,
                    },
                );
                health_mspt_sum = 0.0;
                health_max_mspt = 0.0;
                health_ticks = 0;
                health_start = Instant::now();
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
                        &mut light_in_flight,
                        &mut physics_queue,
                        &mut falling_blocks,
                        &mut falls_pending,
                        &mut projectiles,
                    ) {
                        stop_drain(&mut manager, &light, &tx, &rx, &mut light_in_flight);
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let _ = manager.flush();
                return;
            }
        }
    }
}

// 停机 drain：把还在飞的光照回执收完再落盘
// 没 lit 的区块 flush 会跳过，直接退线程等于把这段改动丢在内存里
// 只处理回执 其它消息一律丢弃：分片已经在退出了 调用方都按超时兜底
fn stop_drain(
    manager: &mut ChunkManager,
    light: &LightService,
    tx: &Sender<RegionMsg>,
    rx: &Receiver<RegionMsg>,
    light_in_flight: &mut usize,
) {
    // 停机消息之前刚生成、还没来得及提交的任务也补上
    let jobs = manager.take_light_jobs(tx);
    *light_in_flight += jobs.len();
    for job in jobs {
        light.submit(job);
    }

    let deadline = Instant::now() + STOP_DRAIN_TIMEOUT;
    while *light_in_flight > 0 {
        let now = Instant::now();
        if now >= deadline {
            warn!(
                in_flight = *light_in_flight,
                "停机时光照没等到回执 已过期的区块这次不落盘"
            );
            break;
        }
        match rx.recv_timeout((deadline - now).min(REGION_TICK)) {
            Ok(RegionMsg::LightDone { receipt }) => {
                manager.apply_light_receipt(receipt);
                *light_in_flight = light_in_flight.saturating_sub(1);
            }
            // 停机路径不再处理业务消息 让投递方按通道断开兜底
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    // 光照回填之后 lit 才为真 这一步决定编辑到底进不进存档
    let _ = manager.flush();
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
            // 视野移走之后队列里还留着旧坐标。属主分片可能已经按空闲回收，
            // 继续投递只会每 tick 报一次警。直接丢掉并清掉已下发记录，
            // 以后重新走进视野时还能再次入队。
            if !within_view_distance(view.chunk_center, pos, view.radius) {
                view.sent_chunks.remove(&pos);
                view.awaiting_light.remove(&pos);
                continue;
            }
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
                    // 光照没回填的先退回队列 等下一轮 LightDone 重排后再发
                    // sent_chunks 保留 入队判重靠它 awaiting_light 标记等光照
                    if manager.is_lit(pos.0, pos.1) {
                        // 真正要发了 摘掉等待标记 后续邻居回执不能再把它重排
                        view.awaiting_light.remove(&pos);
                        owned.push(pos);
                    } else {
                        // 缓存里可能只是装饰顺手铺的邻居 只有地形没有光照任务
                        // 补一次完整流程 不然这里会永远等不到 LightDone
                        manager.prepare_for_light(pos.0, pos.1);
                        view.awaiting_light.insert(pos);
                        view.pending_chunks.push_back(pos);
                    }
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

// 下落方块实体逐 tick 驱动: 重力/落地固化/砸伤/移动同步
// 与掉落物一样实体权威在本线程, global 只广播下行包
#[allow(clippy::too_many_arguments)]
fn tick_falling_blocks(
    manager: &mut ChunkManager,
    physics_queue: &mut UpdateQueue,
    falling: &mut Vec<FallingBlock>,
    directory: &Arc<PlayerDirectory>,
    id: RegionId,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    view_distance: i32,
    now: u64,
) {
    if falling.is_empty() {
        return;
    }

    // 批量查脚下方块 避免每实体一次跨线程往返
    let mut probes = Vec::with_capacity(falling.len() * 2);
    for block in falling.iter() {
        probes.extend_from_slice(&block.probe_columns());
    }
    let blocks = manager.blocks(&probes);

    let players_snapshot = directory.spawned();
    let mut landed: Vec<FallingBlock> = Vec::new();
    let mut moves: Vec<Bytes> = Vec::new();
    let mut damages: Vec<RegionEvent> = Vec::new();
    let mut changes: Vec<CellChange> = Vec::new();
    let mut drop_events: Vec<RegionEvent> = Vec::new();

    for (index, block) in falling.iter_mut().enumerate() {
        let below = blocks[index * 2];
        let ground = blocks[index * 2 + 1];
        let was_falling = block.velocity.1 < 0.0;

        if let Some((bx, by, bz)) = block.step(below, ground) {
            // 哨兵(-1,-1,-1): 落入虚空 只移除实体
            if bx < 0 {
                landed.push(block.clone());
                continue;
            }
            // 落地: 目标格必须是可替换的(空气/液体/植物) 否则方块被挤掉
            let target = manager.block(bx, by, bz);
            let can_place = target == crate::level::generator::blocks::AIR
                || crate::level::generator::blocks::is_overridable(target)
                || crate::level::physics::is_liquid(target);
            if can_place && by >= 0 {
                manager.set_block_meta(bx, by, bz, block.block_id, block.block_data);
                changes.push(CellChange { x: bx, y: by, z: bz, id: block.block_id, meta: block.block_data });
                // 落地唤醒四邻 液体被替换后要重新流动
                physics::on_placed(manager, physics_queue, bx, by, bz, block.block_id, now);
            } else if !block.creative && block.block_id != crate::level::generator::blocks::AIR {
                // 原版 FallingBlock::normalTick: tryToPlace 失败且非创造 -> spawnAtLocation(id, 1)
                // 目标格被占(如落到另一块重力方块上)时 方块变成掉落物 而不是凭空消失
                // region 线程不能自己分配 eid 交给 global 回投 AddDrop
                drop_events.push(RegionEvent::FallingBlockDropped {
                    x: bx,
                    y: by,
                    z: bz,
                    id: block.block_id,
                    meta: block.block_data,
                    creative: block.creative,
                });
            }
            // 砸落伤害: 下落中撞到玩家 或玩家正好站在落点格
            if was_falling {
                for (addr, state) in players_snapshot.iter() {
                    if block.hits_player(state.position) {
                        // 每格最多 2 点 铁砧 40 封顶 这里统一给 2
                        damages.push(RegionEvent::DamagePlayer { addr: *addr, damage: 2 });
                        break;
                    }
                }
            }
            landed.push(block.clone());
        } else {
            // 仍在下落 每 tick 同步位置给客户端(移动包)
            moves.push(
                MoveEntityPacket {
                    entries: vec![crate::network::protocol::move_entity::MoveEntityEntry {
                        eid: block.eid,
                        position: block.position,
                        yaw: 0.0,
                        head_yaw: 0.0,
                        pitch: 0.0,
                    }],
                }
                .encode(),
            );
        }
    }

    // 移除已落地的实体 广播 RemoveEntity
    for block in &landed {
        falling.retain(|f| f.eid != block.eid);
    }
    let mut all_bodies = Vec::new();
    all_bodies.extend(moves);
    for block in &landed {
        all_bodies.push(block.remove_packet());
    }
    if !all_bodies.is_empty() {
        // 用第一个实体的位置做广播中心(SendDrops 同理 实体通常聚在一起)
        let center = landed.first().map(|b| b.position).unwrap_or((0.0, 0.0, 0.0));
        push_outbound(
            outbound,
            RegionOutbound {
                to: None,
                skip: None,
                around: Some((center, view_radius_blocks(view_distance))),
                bodies: all_bodies,
                events: Vec::new(),
                chunks_complete: false,
            },
        );
    }
    if !changes.is_empty() {
        push_outbound(
            outbound,
            RegionOutbound {
                to: None, skip: None, around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::BlockChanged { cells: changes }],
                chunks_complete: false,
            },
        );
    }
    if !damages.is_empty() {
        push_outbound(
            outbound,
            RegionOutbound {
                to: None, skip: None, around: None,
                bodies: Vec::new(),
                events: damages,
                chunks_complete: false,
            },
        );
    }
    if !drop_events.is_empty() {
        push_outbound(
            outbound,
            RegionOutbound {
                to: None, skip: None, around: None,
                bodies: Vec::new(),
                events: drop_events,
                chunks_complete: false,
            },
        );
    }
    let _ = id;
}

// 投射物每拍物理: 推进 + 撞地移除 + MoveEntity 同步
fn tick_projectiles(
    manager: &mut ChunkManager,
    projectiles: &mut Vec<crate::entity::projectile::Projectile>,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    view_distance: i32,
) {
    if projectiles.is_empty() {
        return;
    }
    let mut bodies = Vec::new();
    let mut done = Vec::new();
    for p in projectiles.iter_mut() {
        // 查脚下地面顶面: 从当前位置向下找第一个非 free 方块
        let bx = p.position.0.floor() as i32;
        let bz = p.position.2.floor() as i32;
        let feet = p.position.1 - 0.1;
        let mut ground_top: Option<f32> = None;
        let start_y = feet.floor() as i32;
        for y in (0..=start_y.min(255)).rev() {
            if manager.owns(bx >> 4, bz >> 4) && !crate::entity::falling_block::is_free(manager.block(bx, y, bz)) {
                ground_top = Some((y + 1) as f32);
                break;
            }
        }
        if p.step(ground_top) {
            done.push(p.eid);
            bodies.push(p.remove_packet());
        } else {
            bodies.push(p.move_packet());
        }
    }
    for eid in done {
        projectiles.retain(|p| p.eid != eid);
    }
    if !bodies.is_empty() {
        let center = projectiles.first().map(|p| p.position).unwrap_or((0.0, 5.0, 0.0));
        push_outbound(outbound, RegionOutbound {
            to: None, skip: None,
            around: Some((center, view_radius_blocks(view_distance))),
            bodies, events: Vec::new(), chunks_complete: false,
        });
    }
}

// 饥饿系统每拍推进: 吃冷却递减 + 每 80 tick 回血/饿死 + 疲劳衰减
// 与 ITX Player::onUpdate / 原版 HungerAttributeDelegate 对齐 玩家状态在本线程是权威
fn tick_hunger(
    players: &mut HashMap<SocketAddr, crate::entity::RegionPlayer>,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    ticks: u64,
) {
    for (addr, player) in players.iter_mut() {
        let state = &mut player.state;
        if !state.spawned {
            continue;
        }
        if state.eat_cooldown > 0 {
            state.eat_cooldown -= 1;
        }

        // 药水效果结算: 再生/中毒/饱和/饥饿 每 tick 推进 到期发移除包
        let mut changed = false;
        if !state.effects.is_empty() {
            state.effect_tick = state.effect_tick.wrapping_add(1);
            let (hp, hunger, effect_packets) = crate::entity::effect::tick_effects(
                &mut state.effects,
                state.health,
                state.hunger,
                state.effect_tick,
            );
            if hp != state.health || (hunger - state.hunger).abs() > f32::EPSILON {
                changed = true;
                state.health = hp;
                state.hunger = hunger;
            }
            if !effect_packets.is_empty() {
                let mut bodies: Vec<bytes::Bytes> = vec![crate::item::behavior::hunger_attributes(state)];
                for (id, duration, remove) in effect_packets {
                    let pkt = if remove {
                        crate::network::protocol::mob_effect::MobEffectPacket::remove(state.eid, id)
                    } else {
                        crate::network::protocol::mob_effect::MobEffectPacket::add(
                            state.eid, id, 0, duration,
                        )
                    };
                    bodies.push(pkt.encode());
                }
                push_outbound(outbound, RegionOutbound {
                    to: Some(*addr), skip: None, around: None,
                    bodies, events: Vec::new(), chunks_complete: false,
                });
            }
        }

        // 创造模式不饿不回血
        if state.gamemode & crate::network::GAMEMODE_CREATIVE != 0 {
            continue;
        }

        // 每 80 tick 触发一次回血/饿死 (原版 HEALTH_TICK_COUNT=80)
        if ticks % crate::item::food::HEALTH_TICK_COUNT as u64 == 0 {
            let (hunger, saturation, health, exhaustion) = crate::item::behavior::food_tick(
                state.hunger,
                state.saturation,
                state.health,
                state.exhaustion,
                state.difficulty,
            );
            if (hunger - state.hunger).abs() > f32::EPSILON
                || (saturation - state.saturation).abs() > f32::EPSILON
                || health != state.health
            {
                changed = true;
                state.hunger = hunger;
                state.saturation = saturation;
                state.exhaustion = exhaustion;
                state.health = health;
            }
        }

        if changed {
            // 血量变化下发 SetHealth 饥饿条下发属性包
            let mut bodies = vec![crate::item::behavior::hunger_attributes(state)];
            bodies.push(
                crate::network::protocol::entity_event::SetHealthPacket { health: state.health }.encode(),
            );
            push_outbound(
                outbound,
                RegionOutbound {
                    to: Some(*addr),
                    skip: None,
                    around: None,
                    bodies,
                    events: Vec::new(),
                    chunks_complete: false,
                },
            );
        }
    }
}

// 视距换算成可见半径 视野是按区块给的 广播按区块换算成格
fn view_radius_blocks(view_distance: i32) -> f32 {
    (view_distance * 16) as f32
}

// 这一格能不能被放置覆盖 与 Nukkit 的 block.canBeReplaced() 同义
// 草 雪 花 这类可覆盖方块允许直接顶掉 另有属主校验 双格的门床不能跨分片
// 门铰链位 照 Nukkit BlockDoor::place
// 铰链侧已经有同种门 或 正面是墙而铰链侧是空的 都要带 0x01
fn door_hinged(manager: &mut ChunkManager, x: i32, y: i32, z: i32, direction: u8, id: u8) -> bool {
    let (hinge, facing) = crate::block::placement::door_faces(direction);
    let (hx, hz) = crate::block::placement::face_offset(hinge);
    let (fx, fz) = crate::block::placement::face_offset(facing);
    let next = manager.block(x + hx, y, z + hz);
    let next2 = manager.block(x + fx, y, z + fz);
    next == id
        || (!crate::level::generator::blocks::is_transparent(next2)
        && crate::level::generator::blocks::is_transparent(next))
}

// 挖掘请求的受理结果
enum DigAdmit {
    // 到点了 立刻挖
    Now,
    // 太快 排进队列等下一个允许的 tick
    Queued,
    // 队列也满了 判定异常流量 回补一格
    Reject,
}

// 真正挖一格 连带拆门并推回执给 global
fn dig_block(
    manager: &mut ChunkManager,
    physics_queue: &mut UpdateQueue,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    addr: SocketAddr,
    x: i32,
    y: i32,
    z: i32,
    now: u64,
) {
    // id/meta 要在挖之前读 挖掉之后就是空气了
    let id = manager.block(x, y, z);
    let meta = manager.meta(x, y, z);
    let broken = manager.break_block(x, y, z);
    // 挖掉后唤醒邻居 让重力方块下落/液体回缩
    if broken.is_some() {
        physics::on_broken(manager, physics_queue, x, y, z, now);
    }
    // 门是两格 拆一半另一半也要拆 不然会留下悬空门
    let paired = if broken.is_some() && crate::level::generator::blocks::is_door(id) {
        paired_door(manager, x, y, z, id, meta)
    } else {
        None
    };
    push_block_broken(outbound, addr, x, y, z, id, meta, broken.is_some(), paired);
}

fn push_block_broken(
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    addr: SocketAddr,
    x: i32,
    y: i32,
    z: i32,
    id: u8,
    meta: u8,
    ok: bool,
    paired: Option<(i32, i32, i32, u8, u8)>,
) {
    let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
    buffer.push(RegionOutbound {
        to: None,
        skip: None,
        around: None,
        bodies: Vec::new(),
        events: vec![RegionEvent::BlockBroken {
            addr,
            x,
            y,
            z,
            id,
            meta,
            ok,
            paired,
        }],
        chunks_complete: false,
    });
}

// 每 tick 把排队的挖掘按 MIN_DIG_TICKS 的节奏补上
// 一次最多补一格 保持和"当场受理"完全一样的速度上限
fn drain_digs(
    manager: &mut ChunkManager,
    physics_queue: &mut UpdateQueue,
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    outbound: &Arc<Mutex<Vec<RegionOutbound>>>,
    ticks: u64,
) {
    for (addr, player) in players.iter_mut() {
        while ticks >= player.next_dig_tick {
            let Some((x, y, z)) = player.dig_queue.pop_front() else {
                break;
            };
            player.next_dig_tick = ticks + MIN_DIG_TICKS;
            dig_block(manager, physics_queue, outbound, *addr, x, y, z, ticks);
        }
    }
}

// 拆门连带另一半 Nukkit BlockDoor::onBreak
// 上半(meta 带 0x08)拆下面 下半拆上面 返回另一半的原样给网络层广播
fn paired_door(
    manager: &mut ChunkManager,
    x: i32,
    y: i32,
    z: i32,
    id: u8,
    meta: u8,
) -> Option<(i32, i32, i32, u8, u8)> {
    let other_y = if meta & 0x08 != 0 { y - 1 } else { y + 1 };
    if manager.block(x, other_y, z) != id {
        return None;
    }
    let other_meta = manager.meta(x, other_y, z);
    manager.break_block(x, other_y, z);
    Some((x, other_y, z, id, other_meta))
}

// 放置合法性统一走方块物理模块: 普通方块看目标格是否被占 植物还要看基底
// 这样"高草上叠高草"这类不可能的放置会被拒绝
fn placeable(manager: &mut ChunkManager, x: i32, y: i32, z: i32, placing_id: u8) -> bool {
    physics::can_place(manager, x, y, z, placing_id)
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
    light_in_flight: &mut usize,
    physics_queue: &mut UpdateQueue,
    falling_blocks: &mut Vec<FallingBlock>,
    falls_pending: &mut HashSet<(i32, i32, i32)>,
    projectiles: &mut Vec<crate::entity::projectile::Projectile>,
) -> bool {
    match msg {
        RegionMsg::GetBlock { x, y, z, reply } => {
            let _ = reply.send(manager.block(x, y, z));
        }
        #[cfg(test)]
        RegionMsg::GetBlockMeta { x, y, z, reply } => {
            let _ = reply.send(manager.meta(x, y, z));
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
            Err(_) => {
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
                    if manager.is_lit(chunk_x, chunk_z) {
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
                        // 刚生成/还没算光照：全暗 payload 客户端会当空气不渲染。
                        // 补排光照任务并挂起，等 LightDone 回 writes 后再下发结算。
                        manager.prepare_for_light(chunk_x, chunk_z);
                        io.foreign_lit_wait
                            .entry((chunk_x, chunk_z))
                            .or_default()
                            .push((addr, ack));
                    }
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
                            if manager.is_lit(x, z) {
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
                                io.settle_foreign(addr, registry);
                            } else {
                                // 读盘回来的是新生成区块 lit=false、天空光全暗，
                                // 直接下发同样会空白。补排光照并挂起等回执。
                                manager.prepare_for_light(x, z);
                                let ack = io
                                    .foreign_pending
                                    .get(&addr)
                                    .map(|fp| fp.ack)
                                    .unwrap_or(id);
                                io.foreign_lit_wait
                                    .entry((x, z))
                                    .or_default()
                                    .push((addr, ack));
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
        // 网络层请求开关门 Nukkit BlockDoor::toggle
        // 点上半就翻下半 meta 的 0x04 点下半翻自己 没配对下半就当没开成
        RegionMsg::ToggleDoorFor { addr, x, y, z } => {
            let id = manager.block(x, y, z);
            // 不是门 或没配对下半 静默返回 手持物品点普通方块不该有任何副作用
            if !crate::level::generator::blocks::is_door(id) {
                return true;
            }
            let meta = manager.meta(x, y, z);
            let (lower_y, lower_meta) = if meta & 0x08 != 0 {
                (y - 1, manager.meta(x, y - 1, z))
            } else {
                (y, meta)
            };
            if manager.block(x, lower_y, z) != id {
                return true;
            }
            let next = lower_meta ^ 0x04;
            manager.set_block_meta(x, lower_y, z, id, next);
            let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
            buffer.push(RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::DoorToggled {
                    addr,
                    x,
                    y: lower_y,
                    z,
                    id,
                    meta: next,
                }],
                chunks_complete: false,
            });
        }
        // 熔炉点燃/熄火 保留朝向 meta 广播由 global 那边的 SlotChange::Lit 负责
        RegionMsg::SetFurnaceLit { x, y, z, lit } => {
            let current = manager.block(x, y, z);
            if current != crate::level::generator::blocks::FURNACE
                && current != crate::level::generator::blocks::BURNING_FURNACE
            {
                return true;
            }
            let id = if lit {
                crate::level::generator::blocks::BURNING_FURNACE
            } else {
                crate::level::generator::blocks::FURNACE
            };
            if current != id {
                let meta = manager.meta(x, y, z);
                manager.set_block_meta(x, y, z, id, meta);
            }
        }
        // 点了容器方块 判定窗口类型并回事件 非容器静默
        RegionMsg::OpenContainerFor { addr, x, y, z } => {
            let id = manager.block(x, y, z) as i16;
            let (kind, slots) = match id {
                // Nukkit InventoryType.WORKBENCH 10 格 = 9 合成 + 1 结果
                crate::item::ids::WORKBENCH => (1u8, 10u16),
                // 燃烧中的熔炉(62)也要能打开 用的是同一个容器
                crate::item::ids::FURNACE | 62 => (2u8, 3u16),
                crate::item::ids::CHEST => (0u8, 27u16),
                crate::item::ids::TRAPPED_CHEST => (0u8, 27u16),
                _ => return true,
            };
            let meta = manager.meta(x, y, z);
            let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
            buffer.push(RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::ContainerOpen {
                    addr,
                    window_id: 1,
                    kind,
                    slots,
                    x,
                    y,
                    z,
                    meta,
                }],
                chunks_complete: false,
            });
        }
        // 网络层请求挖方块 挖完把结果作为事件交回去
        // 超速请求不直接拒: 客户端本地已经把方块删了 拒绝就是回弹
        // 先排进队列 到下一个允许的 tick 依次补上 速度上限仍是 MIN_DIG_TICKS
        RegionMsg::BreakBlockFor { addr, x, y, z } => {
            let admit = match players.get_mut(&addr) {
                // 玩家不在本分片(跨分片挖)没有节流状态 直接放行
                None => DigAdmit::Now,
                Some(player) => {
                    if ticks >= player.next_dig_tick {
                        player.next_dig_tick = ticks + MIN_DIG_TICKS;
                        DigAdmit::Now
                    } else if player.dig_queue.len() < MAX_DIG_QUEUE {
                        player.dig_queue.push_back((x, y, z));
                        DigAdmit::Queued
                    } else {
                        DigAdmit::Reject
                    }
                }
            };
            match admit {
                DigAdmit::Now => {
                    let before = manager.block(x, y, z);
                    dig_block(manager, physics_queue, &outbound, addr, x, y, z, ticks);
                    // 生存模式挖掉方块后消耗手持工具 1 点耐久 损坏同步背包
                    if let Some(player) = players.get_mut(&addr)
                        && before != 0 && manager.block(x, y, z) == 0
                        && player.state.gamemode & crate::network::GAMEMODE_CREATIVE == 0
                        && player.state.held_item().is_some_and(|it| crate::item::tools::max_durability(it.id).is_some())
                    {
                        player.state.damage_held_tool();
                        push_outbound(&outbound, RegionOutbound {
                            to: Some(addr), skip: None, around: None,
                            bodies: vec![player.state.inventory_packet().encode()],
                            events: Vec::new(), chunks_complete: false,
                        });
                    }
                }
                DigAdmit::Queued => {}
                // 队列都满了 判定异常流量 回执带上原方块让客户端补回去
                DigAdmit::Reject => {
                    let id = manager.block(x, y, z);
                    let meta = manager.meta(x, y, z);
                    push_block_broken(&outbound, addr, x, y, z, id, meta, false, None);
                }
            }
        }
        // 网络层请求放方块 点击的方块要在 目标格要空 双格的第二格也要空
        RegionMsg::PlaceBlockFor {
            addr,
            click,
            target,
            cells,
            item,
        } => {
            let (cx, cy, cz) = click;
            let (tx, ty, tz) = target;
            let mut cells = cells;
            // 按要放的方块判定合法性 植物必须有基底(杜绝草上叠草)
            let mut ok = manager.block(cx, cy, cz) != 0
                && placeable(manager, tx, ty, tz, cells.first.id);
            if ok {
                if let Some(second) = cells.second {
                    let (sx, sy, sz) = (tx + second.dx, ty + second.dy, tz + second.dz);
                    ok = placeable(manager, sx, sy, sz, second.id);
                }
            }
            // 铰链位要读放置前的邻居 所以放在写入之前算
            if ok
                && cells.second.is_some()
                && crate::level::generator::blocks::is_door(cells.first.id)
                && door_hinged(manager, tx, ty, tz, cells.first.meta, cells.first.id)
            {
                if let Some(second) = cells.second.as_mut() {
                    second.meta |= 0x01;
                }
            }
            if ok {
                manager.set_block_meta(tx, ty, tz, cells.first.id, cells.first.meta);
                physics::on_placed(manager, physics_queue, tx, ty, tz, cells.first.id, ticks);
                if let Some(second) = cells.second {
                    let (sx, sy, sz) = (tx + second.dx, ty + second.dy, tz + second.dz);
                    manager.set_block_meta(sx, sy, sz, second.id, second.meta);
                    physics::on_placed(manager, physics_queue, sx, sy, sz, second.id, ticks);
                }
            }
            let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
            buffer.push(RegionOutbound {
                to: None,
                skip: None,
                around: None,
                bodies: Vec::new(),
                events: vec![RegionEvent::BlockPlaced {
                    addr,
                    origin: target,
                    cells,
                    item,
                    ok,
                }],
                chunks_complete: false,
            });
        }
        // 投射物: 生成实体并广播 AddEntity
        RegionMsg::LaunchProjectile { eid, kind, position, direction, speed } => {
            let p = crate::entity::projectile::Projectile::new(eid, kind, position, direction, speed);
            let center = p.position;
            push_outbound(&outbound, RegionOutbound {
                to: None,
                skip: None,
                around: Some((center, view_radius_blocks(view_distance))),
                bodies: vec![p.add_packet()],
                events: Vec::new(),
                chunks_complete: false,
            });
            projectiles.push(p);
        }
        // 功能物品对方块使用 原子应用方块改动并唤醒物理
        RegionMsg::UseItemOn { addr, item, click, target, slot } => {
            let target_id = manager.block(click.0, click.1, click.2);
            let target_meta = manager.meta(click.0, click.1, click.2);
            let place_block = manager.block(target.0, target.1, target.2);
            let below_solid = target.1 > 0 && !crate::level::generator::blocks::is_transparent(
                manager.block(target.0, target.1 - 1, target.2)
            );
            let result = crate::item::functional::use_item_on(
                item, click, target_id, target_meta, target, place_block, below_solid,
            );
            if let Some(result) = result {
                let mut changes: Vec<crate::level::physics::CellChange> = Vec::new();
                for set in &result.sets {
                    if manager.owns(set.x >> 4, set.z >> 4) {
                        manager.set_block_meta(set.x, set.y, set.z, set.id, set.meta);
                        changes.push(crate::level::physics::CellChange {
                            x: set.x, y: set.y, z: set.z, id: set.id, meta: set.meta,
                        });
                        physics::on_placed(manager, physics_queue, set.x, set.y, set.z, set.id, ticks);
                        // 装液体把源挖空了 唤醒邻居让流动液体回缩
                        if set.id == 0 {
                            physics::on_broken(manager, physics_queue, set.x, set.y, set.z, ticks);
                        }
                    }
                }
                if !changes.is_empty() {
                    push_outbound(&outbound, RegionOutbound {
                        to: None, skip: None, around: None, bodies: Vec::new(),
                        events: vec![RegionEvent::BlockChanged { cells: changes }],
                        chunks_complete: false,
                    });
                }
                // 回写手持物品(满桶/空桶/消耗)与耐久 并同步背包给该玩家
                if let Some(player) = players.get_mut(&addr) {
                    let mut sync = false;
                    match result.replace {
                        Some(Some(new_item)) => {
                            if slot < player.state.inventory.len() {
                                player.state.inventory[slot] = Some(new_item);
                            }
                            sync = true;
                        }
                        Some(None) => {
                            if slot < player.state.inventory.len() {
                                player.state.inventory[slot] = None;
                            }
                            sync = true;
                        }
                        None => {}
                    }
                    if result.damage_tool && player.state.damage_held_tool() {
                        sync = true;
                    }
                    if sync {
                        push_outbound(&outbound, RegionOutbound {
                            to: Some(addr), skip: None, around: None,
                            bodies: vec![player.state.inventory_packet().encode()],
                            events: Vec::new(), chunks_complete: false,
                        });
                    }
                }
            }
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
        // global 分配好 eid 回投 清除重力方块并生成下落实体
        RegionMsg::AddFallingBlock { eid, x, y, z, id: bid, meta, creative } => {
            falls_pending.remove(&(x, y, z));
            // 方块可能在这期间被挖了/被别的实体占了 只有仍是原重力方块才生成
            let still_fallable = manager.block(x, y, z) == bid
                && physics::fall_check(manager, x, y, z).is_some();
            if still_fallable {
                let air = crate::level::generator::blocks::AIR;
                manager.set_block_meta(x, y, z, air, 0);
                let block = FallingBlock::new(eid, bid, meta, (x, y, z), creative);
                let add_packet = block.add_packet();
                let center = block.position;
                // 原版 HeavyBlock::startFalling 里 setBlockAndData(pos, AIR, 3) 的 flags=3
                // 就是 FLAG_ALL: 客户端必须同时收到"原格变空气"才会删掉本地那块方块.
                // 之前只发 AddEntity 导致客户端原地留一块"假沙子" + 下落实体并存.
                // 两条包放同一 bodies 按序下发: 先清方块 再生成实体.
                let clear_packet = UpdateBlockPacket::single(x, y, z, air, 0).encode();
                falling_blocks.push(block);
                push_outbound(outbound, RegionOutbound {
                    to: None,
                    skip: None,
                    around: Some((center, view_radius_blocks(view_distance))),
                    bodies: vec![clear_packet, add_packet],
                    events: Vec::new(),
                    chunks_complete: false,
                });
                // 唤醒四邻 让液体填补/其它重力方块跟进
                physics::on_broken(manager, physics_queue, x, y, z, ticks);
            }
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
        #[cfg(test)]
        RegionMsg::SetBlockForTest { x, y, z, id } => {
            manager.set_block(x, y, z, id);
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
            let mut fresh = RegionPlayer::new(player.state, view_distance);
            // 传送后的纠正标记要跟着走 否则落地第一包就被旧坐标盖掉
            fresh.force_position = player.force_position;
            players.insert(addr, fresh);
        }
        // 服务端发起的传送 照搬 Nukkit Player.teleport
        // 顺序固定：先让客户端知道落点 再改权威坐标与区块队列 最后处理跨分片
        RegionMsg::Teleport {
            addr,
            position,
            yaw,
            pitch,
        } => {
            let Some(player) = players.get(&addr) else {
                return true;
            };
            // 出生流程自己会把位置摆好 没出生不接受传送
            if !player.state.spawned {
                return true;
            }
            let eid = player.state.eid;
            // 权威坐标与下发队列 传送后视野整个重排 朝新位置补区块
            {
                let player = players.get_mut(&addr).expect("刚刚查过");
                player.state.position = position;
                player.state.yaw = yaw;
                player.state.pitch = pitch;
                player.view.update(position, view_distance);
                // 在途的旧移动包还在路上 到位前一律按落点纠正
                player.force_position = Some(position);
            }

            // 跨分片走和玩家自己走过去同一条两段式 目标分片接管并重建视野
            let new_region =
                RegionId::of_block(position.0.floor() as i32, position.2.floor() as i32);
            let (owner, state) = if new_region == id {
                (id, players[&addr].state.clone())
            } else {
                let moved = players.remove(&addr).expect("刚刚查过");
                let delivered = registry.lookup(new_region).is_some_and(|target| {
                    target
                        .send(RegionMsg::Transfer {
                            addr,
                            player: Box::new(moved.clone()),
                            view_distance,
                        })
                        .is_ok()
                });
                let state = moved.state.clone();
                if delivered {
                    (new_region, state)
                } else {
                    warn!(
                        region_x = id.x,
                        region_z = id.z,
                        target_x = new_region.x,
                        target_z = new_region.z,
                        "传送目标分片不可用 玩家留在本分片"
                    );
                    players.insert(addr, moved);
                    (id, state)
                }
            };

            // 落点与归属放进同一批 全局镜像不会晚一拍才更新
            let reset = MovePlayerPacket::teleport(0, position, yaw, pitch).encode();
            push_outbound(
                outbound,
                RegionOutbound {
                    to: Some(addr),
                    skip: None,
                    around: None,
                    bodies: vec![reset],
                    events: vec![RegionEvent::PlayerRegion {
                        addr,
                        region: owner,
                        state: Box::new(state),
                    }],
                    chunks_complete: false,
                },
            );
            // 附近的人只收位移 不用重新 spawn 一次
            let moved = MoveEntityPacket {
                entries: vec![MoveEntityEntry {
                    eid,
                    // 0.14 客户端把移动包里的 y 当眼睛位置
                    position: (position.0, position.1 + EYE_HEIGHT, position.2),
                    yaw,
                    head_yaw: yaw,
                    pitch,
                }],
            }
            .encode();
            push_outbound(
                outbound,
                RegionOutbound {
                    to: None,
                    skip: Some(addr),
                    around: Some((position, view_radius_blocks(view_distance))),
                    bodies: vec![moved],
                    events: Vec::new(),
                    chunks_complete: false,
                },
            );
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
        // 光照算完了 写回缓存 让等它的玩家重新排队下发
        RegionMsg::LightDone { receipt } => {
            // 一个任务一份回执 停机 drain 靠这个计数等所有在途任务落地
            *light_in_flight = light_in_flight.saturating_sub(1);
            let owner = receipt.owner;
            let keys: Vec<(i32, i32)> =
                receipt.chunks.iter().map(|data| (data.x, data.z)).collect();
            manager.apply_light_receipt(receipt);
            for key in keys {
                for player in players.values_mut() {
                    let view = &mut player.view;
                    // 只补弹过队发现没光照的那个窗口：awaiting_light 在退回时插入、
                    // 真正发送时摘除。已发送过的区块不在里面 邻居回执带着它的边缘
                    // 脏数据也不会把已下发的区块重发一遍
                    if view.awaiting_light.remove(&key) && !view.pending_chunks.contains(&key) {
                        view.pending_chunks.push_back(key);
                    }
                }
            }
            // 跨界代发的区块之前因没算光照被挂起 现在目标区块 lit 打开了才真正下发。
            // owner 是任务目标必定已开 lit；keys 里可能只是邻居边界修正，不能单独据此发。
            let mut ready = Vec::new();
            if manager.is_lit(owner.0, owner.1) && io.foreign_lit_wait.contains_key(&owner) {
                ready.push(owner);
            }
            for key in ready {
                if let Some(waiters) = io.foreign_lit_wait.remove(&key) {
                    let data = manager.payload(key.0, key.1);
                    let body = FullChunkDataPacket {
                        chunk_x: key.0,
                        chunk_z: key.1,
                        order: ORDER_COLUMNS,
                        data,
                    }
                    .encode();
                    for (addr, _ack) in waiters {
                        let mut buffer = outbound.lock().unwrap_or_else(|e| e.into_inner());
                        buffer.push(RegionOutbound {
                            to: Some(addr),
                            skip: None,
                            around: None,
                            bodies: vec![body.clone()],
                            events: Vec::new(),
                            chunks_complete: false,
                        });
                        io.settle_foreign(addr, registry);
                    }
                }
            }
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
            LightService::start(),
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
                    if let RegionEvent::BlockBroken { id, ok, .. } = event {
                        return if ok { Some(id) } else { None };
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

    // 同一 tick 里连挖两格 第二下排队到下一个允许的 tick 而不是拒掉
    // 拒掉会让客户端本地已删的方块回弹 所以这里必须等到成功回执
    #[test]
    fn digging_needs_two_ticks_between_breaks() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50097".parse().unwrap();
        // 节流状态挂在玩家身上 分片里得先有这个人
        region.join(addr, player_at((0.5, 5.0, 0.5), crate::config::DEFAULT_VIEW_DISTANCE));
        region.break_block_for(addr, 0, 4, 0);
        region.break_block_for(addr, 1, 4, 0);

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen: Vec<(i32, bool, u8)> = Vec::new();
        while seen.len() < 2 {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockBroken { x, id, ok, .. } = event {
                        seen.push((x, ok, id));
                    }
                }
            }
            assert!(Instant::now() < deadline, "挖掘没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }

        // 第一下立刻生效
        assert_eq!(seen[0], (0, true, GRASS));
        // 第二下排队两拍后照样挖掉 客户端不会看到回弹
        assert_eq!(seen[1], (1, true, GRASS));
        assert_eq!(region.block(1, 4, 0), 0);
    }

    fn player_at(position: (f32, f32, f32), radius: i32) -> RegionPlayer {
        let mut state = PlayerState::default();
        state.position = position;
        RegionPlayer::new(state, radius)
    }

    // global 分配 eid 后回投 属主分片应清除该格方块并广播 AddEntity(type=66)
    // 关键: 必须先发 UpdateBlock(AIR) 再发 AddEntity 否则客户端原位置留"假沙子"
    #[test]
    fn falling_block_spawns_entity_and_clears_block() {
        use crate::network::protocol::pid::{ADD_ENTITY, UPDATE_BLOCK};
        let region = spawn_flat(0, 0);
        // 先在悬空处放一个沙子(下方是空气) y=10
        region.join(
            "127.0.0.1:50096".parse().unwrap(),
            player_at((0.5, 5.0, 0.5), crate::config::DEFAULT_VIEW_DISTANCE),
        );
        // 直接回投一个下落实体(模拟 global 分配 eid=42 后的 AddFallingBlock)
        region.set_block_for_test(0, 10, 0, crate::level::generator::blocks::SAND);
        region.add_falling_block(42, 0, 10, 0, crate::level::generator::blocks::SAND, 0, false);

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut saw_add_entity = false;
        let mut saw_clear_air = false;
        loop {
            for item in region.drain_outbound() {
                for body in item.bodies {
                    if body.first() == Some(&ADD_ENTITY) {
                        saw_add_entity = true;
                        // 校验实体类型是 66
                        assert_eq!(&body[9..13], &66i32.to_be_bytes());
                    }
                    if body.first() == Some(&UPDATE_BLOCK) {
                        // 清方块包: x=0,y=10,z=0,id=0(AIR)
                        assert_eq!(&body[5..9], &0i32.to_be_bytes()); // x
                        assert_eq!(&body[9..13], &0i32.to_be_bytes()); // z
                        assert_eq!(body[13], 10); // y
                        assert_eq!(body[14], 0); // block id AIR
                        saw_clear_air = true;
                    }
                }
            }
            if saw_add_entity && saw_clear_air {
                break;
            }
            assert!(Instant::now() < deadline, "下落实体/清方块包没有生成");
            std::thread::sleep(Duration::from_millis(10));
        }
        // 原格应被清空
        assert_eq!(region.block(0, 10, 0), 0);
    }

    // 停机 drain：还没照亮就发生的编辑不能在退线程时丢掉
    // 这里直接调 stop_drain 因为走完整分片时异步光照往往已经先算完了
    #[test]
    fn stop_drain_flushes_edits_on_unlit_chunks() {
        use crate::level::chunk_loader::ChunkLoader;

        let path = std::env::temp_dir().join(format!(
            "violin-drain-{}-{}.redb",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let loader = Arc::new(ChunkLoader::open(&path).unwrap());
        let mut manager = ChunkManager::with_shared_loader(loader.clone());

        // 生成区块并挖掉一格 这时光照还没算 lit=false
        assert_eq!(manager.block(0, 4, 0), GRASS);
        assert_eq!(manager.break_block(0, 4, 0), Some(GRASS));
        assert!(!manager.is_lit(0, 0));

        let light = LightService::start();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut in_flight = 0;
        stop_drain(&mut manager, &light, &tx, &rx, &mut in_flight);
        assert_eq!(in_flight, 0, "回执都该收完");

        // 重新打开存档确认落的是挖过之后的方块
        let mut reopened = ChunkManager::with_shared_loader(loader);
        assert_eq!(reopened.block(0, 4, 0), 0, "停机前没等到光照 编辑丢了");

        let _ = std::fs::remove_file(&path);
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

    // 门是两格 下半朝向下半 8 上半只有 0x08 两格都要落盘且都带 meta
    #[test]
    fn door_writes_two_cells_with_meta() {
        let region = spawn_flat(0, 0);
        assert_eq!(region.block(0, 4, 0), GRASS);

        let cells = crate::block::placement::Placement {
            first: crate::block::placement::Cell {
                dx: 0,
                dy: 0,
                dz: 0,
                id: crate::level::generator::blocks::WOOD_DOOR_BLOCK,
                meta: 2,
            },
            second: Some(crate::block::placement::Cell {
                dx: 0,
                dy: 1,
                dz: 0,
                id: crate::level::generator::blocks::WOOD_DOOR_BLOCK,
                meta: 0x08,
            }),
        };
        let addr: std::net::SocketAddr = "127.0.0.1:50096".parse().unwrap();
        region.place_cells_for(addr, (0, 4, 0), (0, 5, 0), cells);
        // 等回执
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let mut done = false;
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockPlaced { ok, .. } = event {
                        assert!(ok);
                        done = true;
                    }
                }
            }
            if done {
                break;
            }
            assert!(Instant::now() < deadline, "放置没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(region.block(0, 5, 0), crate::level::generator::blocks::WOOD_DOOR_BLOCK);
        assert_eq!(region.block(0, 6, 0), crate::level::generator::blocks::WOOD_DOOR_BLOCK);
        assert_eq!(region.meta(0, 5, 0), 2);
        assert_eq!(region.meta(0, 6, 0), 0x08);
    }

    fn door_cells(meta: u8) -> crate::block::placement::Placement {
        use crate::block::placement::{Cell, Placement};
        let id = crate::level::generator::blocks::WOOD_DOOR_BLOCK;
        Placement {
            first: Cell { dx: 0, dy: 0, dz: 0, id, meta },
            second: Some(Cell { dx: 0, dy: 1, dz: 0, id, meta: 0x08 }),
        }
    }

    // 等一条放置回执
    fn wait_placed(region: &RegionHandle) -> (bool, crate::block::placement::Placement) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockPlaced { ok, cells, .. } = event {
                        return (ok, cells);
                    }
                }
            }
            assert!(Instant::now() < deadline, "放置没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // 等一条挖掘回执
    fn wait_broken(region: &RegionHandle) -> (bool, Option<(i32, i32, i32, u8, u8)>) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::BlockBroken { ok, paired, .. } = event {
                        return (ok, paired);
                    }
                }
            }
            assert!(Instant::now() < deadline, "挖掘没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // 等一条开关门回执
    fn wait_toggled(region: &RegionHandle) -> (i32, u8) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for item in region.drain_outbound() {
                for event in item.events {
                    if let RegionEvent::DoorToggled { y, meta, .. } = event {
                        return (y, meta);
                    }
                }
            }
            assert!(Instant::now() < deadline, "开关门没有回执");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // 点门开关 翻下半 meta 的 0x04 Nukkit BlockDoor::toggle
    #[test]
    fn toggling_door_flips_open_bit() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50110".parse().unwrap();
        region.place_cells_for(addr, (0, 4, 0), (0, 5, 0), door_cells(0));
        assert!(wait_placed(&region).0);
        assert_eq!(region.meta(0, 5, 0), 0);

        // 点下半 开
        region.toggle_door_for(addr, 0, 5, 0);
        let (y, meta) = wait_toggled(&region);
        assert_eq!((y, meta), (5, 0x04));
        assert_eq!(region.meta(0, 5, 0), 0x04);

        // 再点一次 关
        region.toggle_door_for(addr, 0, 5, 0);
        let (_, meta) = wait_toggled(&region);
        assert_eq!(meta, 0);
        assert_eq!(region.meta(0, 5, 0), 0);
    }

    // 点上半也是翻下半 回执报告的是下半坐标 Nukkit BlockDoor::toggle
    #[test]
    fn toggling_door_upper_half_targets_lower() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50111".parse().unwrap();
        region.place_cells_for(addr, (0, 4, 0), (0, 5, 0), door_cells(0));
        assert!(wait_placed(&region).0);

        region.toggle_door_for(addr, 0, 6, 0);
        let (y, meta) = wait_toggled(&region);
        assert_eq!((y, meta), (5, 0x04));
        assert_eq!(region.meta(0, 5, 0), 0x04);
    }

    // 铰链侧已经有门 第二扇的上半要带 0x01 Nukkit BlockDoor::place
    #[test]
    fn door_hinge_follows_neighbor() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50100".parse().unwrap();
        // 朝向 0(南) 铰链侧是北 z-1 第一扇放 (3,5,3) 正好在第二扇 (3,5,4) 的铰链侧
        // 都放在 chunk 0 内 跨分片的目标格 placeable 会拒
        region.place_cells_for(addr, (3, 4, 3), (3, 5, 3), door_cells(0));
        assert!(wait_placed(&region).0);
        region.place_cells_for(addr, (3, 4, 4), (3, 5, 4), door_cells(0));
        let (ok, cells) = wait_placed(&region);

        assert!(ok);
        assert_eq!(cells.second.unwrap().meta, 0x08 | 0x01);
        assert_eq!(region.meta(3, 6, 3), 0x08);
        assert_eq!(region.meta(3, 6, 4), 0x08 | 0x01);
    }

    // 拆门的任意一半 另一半也要拆掉 Nukkit BlockDoor::onBreak
    #[test]
    fn breaking_door_removes_both_halves() {
        let region = spawn_flat(0, 0);
        let addr: std::net::SocketAddr = "127.0.0.1:50101".parse().unwrap();
        region.place_cells_for(addr, (0, 4, 0), (0, 5, 0), door_cells(0));
        assert!(wait_placed(&region).0);

        region.break_block_for(addr, 0, 5, 0);
        let (ok, paired) = wait_broken(&region);

        assert!(ok);
        assert_eq!(
            paired,
            Some((0, 6, 0, crate::level::generator::blocks::WOOD_DOOR_BLOCK, 0x08))
        );
        assert_eq!(region.block(0, 5, 0), 0);
        assert_eq!(region.block(0, 6, 0), 0);
    }

    #[test]
    fn payload_reflects_edits() {
        let region = spawn_flat(0, 0);
        // 先触发生成让区块进缓存 payload 只回已照亮(lit)的区块
        assert_eq!(region.block(0, 4, 0), GRASS);
        // 光照异步算 轮询等 LightDone 回填
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if region
                .payload(0, 0)
                .map(|data| data[5])
                .is_some_and(|value| value == 0)
            {
                break;
            }
            assert!(Instant::now() < deadline, "光照没有按时回填");
            std::thread::sleep(Duration::from_millis(10));
        }
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
