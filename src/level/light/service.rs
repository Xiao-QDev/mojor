// 多线程异步光照服务
// 快照抓取必须在 region 线程做（ChunkManager 是 region 私有的），
// 所以任务自带快照整体 move 给 worker；worker 池并行消费，算完把
// LightDone 发回发起 region 的消息队列，与 ChunkIoPool 同一套"提交+回执"模式。
//
// 并行安全性（已核对）：
//  - Engine::init / update_blocks 是纯函数：只读任务自带的 Snapshot，
//    模块内无任何 static / thread_local / 共享可变状态（见 light 模块 grep）。
//  - 回执 LightDone 一律发回发起 region，由该 region 线程单线程 apply_light_receipt，
//    不跨线程碰 ChunkManager。
//  - 同一区块 Init 回执到达前不会排 Update（take_light_jobs 里 light_init_in_flight
//    守卫），所以同一区块的任务不会并行乱序；跨区块任务本就互不依赖。
// 因此把原来的单 worker 扩成线程池只提升吞吐，不改变任何语义。
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use crate::level::actor::RegionMsg;

use super::snapshot::Snapshot;

// 一次方块改动 光照增量修正的最小单位 坐标是世界坐标
pub struct BlockChange {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub old_id: u8,
    pub new_id: u8,
}

pub enum LightKind {
    // 新区块全量光照（生成路径）
    Init(Snapshot),
    // 玩家改方块的增量修正
    Update {
        snap: Snapshot,
        changes: Vec<BlockChange>,
    },
}

pub(crate) struct LightJob {
    pub kind: LightKind,
    // 回执发回发起 region 的消息队列
    pub reply: Sender<RegionMsg>,
}

pub struct LightService {
    tx: Sender<LightJob>,
    _workers: Vec<JoinHandle<()>>,
}

impl LightService {
    // 默认 worker 数：跟随可用逻辑处理器，封顶 8。
    // 光照是重 CPU 任务，超过核心数只会增加调度/缓存抖动；封顶也避免
    // 高核数机器上一次性拉起过多线程。可用 VIOLIN_LIGHT_WORKERS 覆盖。
    fn default_workers() -> usize {
        if let Ok(v) = std::env::var("VIOLIN_LIGHT_WORKERS")
            && let Ok(n) = v.parse::<usize>()
            && n > 0
        {
            return n;
        }
        let cpu = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        cpu.clamp(1, 8)
    }

    // 启动 worker 池 所有 region 共享一个服务 任务里带各自的回执通道。
    // 保留无参签名，既有调用点（Level/测试）无需改动。
    pub fn start() -> Arc<Self> {
        Self::start_with_workers(Self::default_workers())
    }

    // 指定 worker 数启动（测试或需要精确控制时使用）
    pub fn start_with_workers(workers: usize) -> Arc<Self> {
        let workers = workers.max(1);
        let (tx, rx) = mpsc::channel::<LightJob>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        let mut handles = Vec::with_capacity(workers);
        for i in 0..workers {
            let rx = Arc::clone(&rx);
            handles.push(
                thread::Builder::new()
                    .name(format!("light-engine-{i}"))
                    .spawn(move || loop {
                        // 只在取任务时短暂持锁；拿到任务立刻释放锁再计算，
                        // 多个 worker 才能真正在不同核心上并行。
                        let job = {
                            let guard = match rx.lock() {
                                Ok(g) => g,
                                Err(_) => break,
                            };
                            guard.recv()
                        };
                        match job {
                            Ok(job) => {
                                let receipt = match job.kind {
                                    LightKind::Init(snap) => {
                                        super::engine::Engine::init(snap).into_receipt()
                                    }
                                    LightKind::Update { snap, changes } => {
                                        super::engine::Engine::update_blocks(snap, &changes)
                                            .into_receipt()
                                    }
                                };
                                // 分片停机时会 drain 在途任务 回执必须送出去
                                // 送不出去说明 region 已经没了 那批区块也没落盘 不算脏数据
                                let _ = job.reply.send(RegionMsg::LightDone { receipt });
                            }
                            // 所有发送端丢弃 线程退出
                            Err(_) => break,
                        }
                    })
                    .expect("启动光照 worker 失败"),
            );
        }
        Arc::new(Self {
            tx,
            _workers: handles,
        })
    }

    // 提交任务 非阻塞
    pub(crate) fn submit(&self, job: LightJob) {
        let _ = self.tx.send(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    // 并行正确性核心不变式：同一批快照用单 worker 与多 worker 计算，
    // 回执（owner + 每块 sky/block 光照 nibble）必须逐字节一致。
    // 光照是纯函数，并行只改变调度顺序，不应改变任何结果。
    #[test]
    fn parallel_matches_serial_results() {
        use crate::level::chunk_manager::ChunkManager;
        use crate::level::generator::Generator;

        // 造一批有真实地形的快照（terrain 生成器比 realistic 快，聚焦光照确定性）
        let coords: Vec<(i32, i32)> = (-3..3).flat_map(|x| (-2..2).map(move |z| (x, z))).collect();
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::terrain(4242));
        for &(x, z) in &coords {
            let _ = manager.get(x, z);
        }
        fn run(coords: &[(i32, i32)], manager: &ChunkManager, workers: usize)
            -> Vec<(i32, i32, Vec<(i32, i32, Vec<u8>, Vec<u8>)>)>
        {
            let pool = LightService::start_with_workers(workers);
            let mut receivers = Vec::new();
            for &(x, z) in coords {
                let (tx, rx) = mpsc::channel();
                receivers.push(rx);
                let snap = Snapshot::grab(manager, x, z);
                pool.submit(LightJob { kind: LightKind::Init(snap), reply: tx });
            }
            let mut out = Vec::new();
            for rx in receivers {
                match rx.recv_timeout(std::time::Duration::from_secs(20)) {
                    Ok(RegionMsg::LightDone { receipt }) => {
                        let chunks = receipt.chunks.into_iter()
                            .map(|c| (c.x, c.z, c.sky_light, c.block_light))
                            .collect();
                        out.push((receipt.owner.0, receipt.owner.1, chunks));
                    }
                    Ok(_) => panic!("意外的消息类型"),
                    Err(e) => panic!("回执超时: {e}"),
                }
            }
            out.sort();
            out
        }

        // 每轮都从同一个（只读）manager 重新 grab，快照内容逐字节一致
        let serial = run(&coords, &manager, 1);
        let parallel = run(&coords, &manager, 8);
        assert_eq!(serial.len(), parallel.len(), "回执数量不一致");
        for (s, p) in serial.iter().zip(parallel.iter()) {
            assert_eq!(s, p, "并行与串行光照结果不一致 owner={:?}", s.0);
        }
    }

    // 吞吐观测（默认 ignored，手动 --ignored 运行）：对比 1/4/8 worker
    // 完成同一批重光照任务的总耗时，验证确实吃满多核。
    #[test]
    #[ignore]
    fn bench_parallel_throughput() {
        use crate::level::chunk_manager::ChunkManager;
        use crate::level::generator::Generator;
        use std::time::Instant;

        let coords: Vec<(i32, i32)> = (-8..8).flat_map(|x| (-8..8).map(move |z| (x, z))).collect();
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::realistic(99));
        for &(x, z) in &coords { let _ = manager.get(x, z); }

        for workers in [1usize, 4, 8] {
            let pool = LightService::start_with_workers(workers);
            let mut receivers = Vec::new();
            let t = Instant::now();
            for &(x, z) in &coords {
                let (tx, rx) = mpsc::channel();
                receivers.push(rx);
                let snap = Snapshot::grab(&manager, x, z);
                pool.submit(LightJob { kind: LightKind::Init(snap), reply: tx });
            }
            for rx in &receivers {
                let _ = rx.recv_timeout(std::time::Duration::from_secs(60));
            }
            eprintln!("workers={workers}: {:?} for {} chunks", t.elapsed(), coords.len());
        }
    }

    // 并行池必须把每个任务的回执正确送回各自的通道，且结果正确。
    // 用多个通道并发提交 Init，验证没有串台/丢失。
    #[test]
    fn worker_pool_returns_receipts_to_own_channel() {
        use crate::level::chunk_manager::ChunkManager;

        let pool = LightService::start_with_workers(4);

        const N: usize = 24;
        let manager = ChunkManager::new();
        let mut receivers = Vec::new();
        for i in 0..N {
            let (tx, rx) = mpsc::channel();
            receivers.push(rx);
            let cx = (i as i32) * 2;
            let snap = Snapshot::grab(&manager, cx, 0);
            pool.submit(LightJob {
                kind: LightKind::Init(snap),
                reply: tx,
            });
        }
        for (i, rx) in receivers.iter().enumerate() {
            match rx.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(RegionMsg::LightDone { receipt }) => {
                    assert_eq!(receipt.owner, ((i as i32) * 2, 0), "回执串台");
                }
                Ok(_) => panic!("收到非 LightDone 消息"),
                Err(e) => panic!("回执丢失 i={i}: {e}"),
            }
        }
    }
}
