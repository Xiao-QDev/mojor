use std::pin::pin;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{info, warn};

use crate::error::Result;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::generator::Generator;
use crate::network::{NetworkState, SendCmd};
use crate::raknet::server::socket::NetEvent;

pub const TICK_MS: u64 = 50;
// 每 10 秒把改过的区块写进 page cache
pub const SAVE_INTERVAL_TICKS: u64 = 200;
// 后台线程的刷盘间隔 比存档周期略长 保证每批写入之后都会被刷到
// 崩溃最多丢这一个周期的改动
pub const SYNC_INTERVAL: Duration = Duration::from_secs(12);

pub async fn run(
    mut net_rx: mpsc::Receiver<NetEvent>,
    cmd_tx: mpsc::Sender<SendCmd>,
    gamemode: u8,
    storage: ChunkLoader,
    generator: Generator,
    seed: u32,
    view_distance: i32,
    io_workers: usize,
) -> Result<()> {
    let mut ticker = interval(Duration::from_millis(TICK_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // 停机信号只创建一次 避免每轮重建 future 丢失信号
    let shutdown = tokio::signal::ctrl_c();
    let mut shutdown = pin!(shutdown);

    let mut tick_count: u64 = 0;
    let mut pending: Vec<NetEvent> = Vec::with_capacity(64);
    let mut network =
        NetworkState::with_loader(gamemode, storage, generator, seed, view_distance, io_workers);
    // 刷盘放到后台 主循环只把改动写进 page cache 不会被磁盘阻塞
    let (sync_stop_tx, sync_stop_rx) = std_mpsc::channel::<()>();
    let sync_thread = network.sync_handle().and_then(|loader| {
        std::thread::Builder::new()
            .name("world-sync".into())
            .spawn(move || {
                loop {
                    match sync_stop_rx.recv_timeout(SYNC_INTERVAL) {
                        Ok(()) | Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std_mpsc::RecvTimeoutError::Timeout) => {
                            if let Err(e) = loader.sync() {
                                warn!(error = %e, "后台刷盘失败");
                            }
                        }
                    }
                }
            })
            .ok()
    });

    'main: loop {
        tokio::select! {
            _ = ticker.tick() => {
                tick_count += 1;

                // 网络事件只在 tick 边界批量取出 保证状态变更顺序确定
                pending.clear();
                while let Ok(evt) = net_rx.try_recv() {
                    pending.push(evt);
                }

                for evt in &pending {
                    match evt {
                        NetEvent::SessionOpened { .. } => {}
                        NetEvent::SessionClosed { addr, reason } => {
                            info!(%addr, reason = %reason, "RakNet 会话关闭");
                            for cmd in network.on_session_closed(addr) {
                                if cmd_tx.try_send(cmd).is_err() {
                                    warn!(%addr, "下行命令队列已满 丢弃玩家下线通知");
                                }
                            }
                        }
                        NetEvent::GamePacket { addr, payload } => {
                            for cmd in network.handle_game_packet(*addr, payload) {
                                if cmd_tx.try_send(cmd).is_err() {
                                    warn!(%addr, "下行命令队列已满 丢弃回包");
                                }
                            }
                        }
                    }
                }

                // 网络状态机的周期工作 目前是分批下发区块
                for cmd in network.tick().await {
                    if cmd_tx.try_send(cmd).is_err() {
                        warn!("下行命令队列已满 丢弃区块批次");
                    }
                }

                if tick_count.is_multiple_of(SAVE_INTERVAL_TICKS) {
                    // 异步投递 不阻塞主循环 region 自己分批编码写盘
                    network.flush_world_async();
                }
            }
            _ = &mut shutdown => {
                info!("Stop!!!");
                break 'main;
            }
        }
    }

    // 停机前把剩下的改动写进存档 先停后台线程再做最后一次刷盘
    match network.flush_world() {
        Ok(n) if n > 0 => info!(chunks = n, "停机前保存世界"),
        Ok(_) => {}
        Err(e) => warn!(error = %e, "停机前保存世界失败"),
    }
    let _ = sync_stop_tx.send(());
    if let Some(handle) = sync_thread {
        let _ = handle.join();
    }
    if let Err(e) = network.sync_world() {
        warn!(error = %e, "停机前刷盘失败");
    }

    info!(tick = tick_count, "主循环结束");
    Ok(())
}


