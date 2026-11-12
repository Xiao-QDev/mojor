use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use crate::level::actor::RegionMsg;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::format::mojoregion::blob;

// 区块 I/O 线程池 把同步的磁盘读 + 解码从 region 线程挪出来
// 照搬 vanilla 的 chunk I/O executor：region 线程只管世界逻辑 加载在后台并行跑
// 多个 region 共享同一个池 落盘区块的读取不再阻塞任何玩家的 tick
// 一个区块的加载完成通过发起 region 自己的消息通道回执 不跨线程碰 ChunkManager
pub struct ChunkIoPool {
    loader: Option<Arc<ChunkLoader>>,
    tx: Sender<IoJob>,
    _workers: Vec<JoinHandle<()>>,
}

struct IoJob {
    x: i32,
    z: i32,
    loader: Option<Arc<ChunkLoader>>,
    reply: Sender<RegionMsg>,
}

impl ChunkIoPool {
    // workers 个后台线程从队列取任务 没有存档时传 None 让 region 现场生成
    pub fn new(loader: Option<Arc<ChunkLoader>>, workers: usize) -> Arc<Self> {
        let (tx, rx) = mpsc::channel::<IoJob>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        let mut handles = Vec::with_capacity(workers.max(1));
        for _ in 0..workers.max(1) {
            let rx = rx.clone();
            handles.push(thread::spawn(move || loop {
                let job = rx.lock().unwrap().recv();
                match job {
                    Ok(job) => {
                        // 唯一的阻塞点 磁盘读 + 解压 + 反序列化 全在这里 不在任何 region 线程
                        let chunk = job
                            .loader
                            .as_ref()
                            .and_then(|loader| loader.load(job.x, job.z).ok().flatten())
                            .and_then(|data| blob::decode(job.x, job.z, &data));
                        let _ = job.reply.send(RegionMsg::ChunkLoaded {
                            x: job.x,
                            z: job.z,
                            chunk,
                        });
                    }
                    // 所有发送端丢弃 线程退出
                    Err(_) => break,
                }
            }));
        }
        Arc::new(Self {
            loader,
            tx,
            _workers: handles,
        })
    }

    // 把一个区块的加载任务交给后台 完成通过发起 region 的通道回执
    // 没有存档时直接回 None 让 region 线程现场生成 不进池
    pub(crate) fn submit(&self, x: i32, z: i32, reply: Sender<RegionMsg>) {
        let Some(loader) = self.loader.clone() else {
            let _ = reply.send(RegionMsg::ChunkLoaded { x, z, chunk: None });
            return;
        };
        let _ = self
            .tx
            .send(IoJob { x, z, loader: Some(loader), reply });
    }
}
