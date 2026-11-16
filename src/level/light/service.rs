// tokio 异步光照服务
// 快照抓取必须在 region 线程做（ChunkManager 是 region 私有的），
// 所以任务自带快照整体 move 给 worker；worker 是独立线程上的单线程
// tokio runtime，串行消费保序；算完把 LightDone 发回发起 region 的
// 消息队列，与 ChunkIoPool 是同一套"提交 + 回执"模式。
use std::sync::Arc;
use std::thread;
use std::sync::mpsc::Sender;

use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

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
    tx: UnboundedSender<LightJob>,
}

impl LightService {
    // 启动 worker 线程 所有 region 共享一个服务 任务里带各自的回执通道
    pub fn start() -> Arc<Self> {
        let (tx, mut rx) = unbounded_channel::<LightJob>();
        thread::Builder::new()
            .name("light-engine".into())
            .spawn(move || {
                // worker 自带一个单线程 runtime：recv().await 空闲时线程挂起，
                // 任务到达后在本线程同步算完（纯 CPU 无 await 点）天然串行保序
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("光照 worker 的 tokio runtime");
                runtime.block_on(async move {
                    while let Some(job) = rx.recv().await {
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
                });
            })
            .expect("启动光照 worker 失败");
        Arc::new(Self { tx })
    }

    // 提交任务 非阻塞
    pub(crate) fn submit(&self, job: LightJob) {
        let _ = self.tx.send(job);
    }
}
