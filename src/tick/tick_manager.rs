use std::pin::pin;
use std::sync::mpsc as std_mpsc;
use std::time::Instant;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    world_name: String,
    max_players: u32,
) -> Result<()> {
    let mut ticker = interval(Duration::from_millis(TICK_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // 停机信号只创建一次 避免每轮重建 future 丢失信号
    let shutdown = tokio::signal::ctrl_c();
    let mut shutdown = pin!(shutdown);

    let mut tick_count: u64 = 0;
    // 每拍开始的时间戳 gap 与 MSPT 都从这里算 统计口径见 tick::metrics
    let mut last_tick = Instant::now();
    let mut pending: Vec<NetEvent> = Vec::with_capacity(64);
    let mut network = NetworkState::with_loader(
        gamemode,
        storage,
        generator,
        seed,
        view_distance,
        io_workers,
        world_name,
    );
    network.set_max_players(max_players);

    // 控制台命令 照搬 Nukkit CommandReader: 独立线程读 stdin 不阻塞 tokio
    // TTY 下支持 Tab 自动补全与行编辑(见 crate::console)
    let (console_tx, mut console_rx) = mpsc::channel::<String>(16);
    std::thread::Builder::new()
        .name("console".into())
        .spawn(move || crate::console::run(console_tx))
        .ok();

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
                            let sync_start = Instant::now();
                            let _ = loader.sync();
                            let sync_ms = sync_start.elapsed().as_millis();
                            if sync_ms > 100 {
                                warn!(sync_ms, "慢帧诊断(后台 sync/fsync)");
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
                // 实际间隔比标称多出来的部分是上一拍拖的时间
                let phase_start = Instant::now();
                let gap = last_tick.elapsed();
                last_tick = Instant::now();
                tick_count += 1;

                // 网络事件只在 tick 边界批量取出 保证状态变更顺序确定
                pending.clear();
                while let Ok(evt) = net_rx.try_recv() {
                    pending.push(evt);
                }
                let after_net_recv = phase_start.elapsed();

                for evt in &pending {
                    match evt {
                        NetEvent::SessionOpened { id, addr, .. } => {
                            network.on_session_opened(*id, addr);
                        }
                        NetEvent::SessionClosed { id, addr, reason } => {
                            // 旧会话退役时新连接可能已经接管该端点 按 id 判断归属
                            for cmd in network.on_session_closed(*id, addr, reason) {
                                let _ = cmd_tx.try_send(cmd);
                            }
                        }
                        NetEvent::GamePacket { id, addr, payload } => {
                            // 旧会话的残留包丢弃 不能打进新会话
                            if !network.session_matches(*id, addr) {
                                continue;
                            }
                            for cmd in network.handle_game_packet(*addr, payload) {
                                match cmd {
                                    // 玩家聊天的服务端镜像 直接打到控制台 不发给 RakNet
                                    SendCmd::Console(line) => info!("{line}"),
                                    other => {
                                        let _ = cmd_tx.try_send(other);
                                    }
                                }
                            }
                        }
                    }
                }

                let after_packets = phase_start.elapsed();

                // 网络状态机的周期工作 目前是分批下发区块
                for cmd in network.tick().await {
                    let _ = cmd_tx.try_send(cmd);
                }
                let after_net_tick = phase_start.elapsed();

                if tick_count.is_multiple_of(SAVE_INTERVAL_TICKS) {
                    // 异步投递 不阻塞主循环 region 自己分批编码写盘
                    network.flush_world_async();
                }

                // 系统性能采样在 tick::os 的独立线程里跑 主循环不参与

                // 本拍耗时就是 MSPT 到这里本拍的活干完了 后面等的是下一拍的空闲
                let mspt = last_tick.elapsed();
                // 慢帧诊断：平均 MSPT 很低但偶发近 1 秒尖峰时，打出各阶段耗时定位卡点。
                // 只在明显超过一拍预算时打印，正常帧零开销、不刷屏。
                if gap.as_millis() > 100 || mspt.as_millis() > 100 {
                    warn!(
                        gap_ms = gap.as_millis() as u64,
                        mspt_ms = mspt.as_millis() as u64,
                        recv_ms = after_net_recv.as_millis() as u64,
                        packets_ms = after_packets.saturating_sub(after_net_recv).as_millis() as u64,
                        net_tick_ms = after_net_tick.saturating_sub(after_packets).as_millis() as u64,
                        "慢帧诊断"
                    );
                }
                network.record_tick(gap, mspt);
            }
            Some(line) = console_rx.recv() => {
                let output = network.run_command(&line);
                for message in &output.messages {
                    info!("{message}");
                }
                for cmd in output.sync {
                    let _ = cmd_tx.try_send(cmd);
                }
            }
            _ = &mut shutdown => {
                if !network.is_shutdown_requested() {
                    const A: &[&str] = &[
                        "Stop! Hah... please try not to use Ctrl+C to stop the server next time, okay?",
                        "Could you please not use Ctrl+C? ...It's a bad habit.",
                        "I really don't get what you're thinking. Just don't press it next time, okay?",
                        "Hah... suit yourself.",
                        "oh my god...",
                        "Ctrl+C is a graceful shutdown, but I still wouldn't recommend it.",
                        "NO!NO!NO!"
                    ];
                    let n = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .subsec_nanos() as usize
                        % A.len();
                    info!("{}", A[n]);
                }
                break 'main;
            }
        }

        // stop 命令从控制台或玩家触发 据此优雅停机
        if network.is_shutdown_requested() {
            break 'main;
        }
    }

    // 停机前把在线玩家和剩下的区块改动都写进存档
    // Ctrl+C 关服不会走 SessionClosed 必须在这里兜底存玩家
    network.save_all_players();
    let _ = network.flush_world();
    let _ = sync_stop_tx.send(());
    if let Some(handle) = sync_thread {
        let _ = handle.join();
    }
    let _ = network.sync_world();

    Ok(())
}


