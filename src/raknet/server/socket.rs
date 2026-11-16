use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};
use tracing::warn;

use crate::config::Config;
use crate::error::Result;
use crate::network::SendCmd;
use crate::raknet::protocol::packets::{
    ID_OPEN_CONNECTION_REQUEST_1, ID_UNCONNECTED_PING, ID_UNCONNECTED_PING_OPEN_CONNECTIONS,
    UnconnectedPing, UnconnectedPong,
};
use crate::raknet::server::session::{Session, SessionEvent};

pub const MAX_DATAGRAM: usize = u16::MAX as usize;
pub const SESSION_TICK_MS: u64 = 50;

// 事件都带会话 id 上层据此区分同端点上的新旧连接
#[derive(Debug)]
pub enum NetEvent {
    SessionOpened {
        id: u64,
        addr: SocketAddr,
        client_id: u64,
    },
    SessionClosed {
        id: u64,
        addr: SocketAddr,
        reason: String,
    },
    GamePacket {
        id: u64,
        addr: SocketAddr,
        payload: Bytes,
    },
}

// 连接任务收到的消息
enum ConnCmd {
    Datagram(Bytes),
    Downlink { bodies: Vec<Bytes> },
    // 同端点新连接接管 旧会话立即退役
    Shutdown,
}

struct ConnHandle {
    id: u64,
    tx: mpsc::Sender<ConnCmd>,
    // 登录完成前不下发游戏包 旧会话在途的残包不会打到新连接
    ready: bool,
}

pub struct RakNetServer {
    socket: Arc<UdpSocket>,
    server_id: u64,
    server_name: Arc<str>,
    net_tx: mpsc::Sender<NetEvent>,
    cmd_rx: mpsc::Receiver<SendCmd>,
    sessions: HashMap<SocketAddr, ConnHandle>,
    next_id: u64,
}

impl RakNetServer {
    pub fn new(
        socket: Arc<UdpSocket>,
        config: &Config,
        net_tx: mpsc::Sender<NetEvent>,
        server_id: u64,
        cmd_rx: mpsc::Receiver<SendCmd>,
    ) -> Self {
        Self {
            socket,
            server_id,
            // pong 名称启动时算一次 之后每个 ping 不再分配字符串
            server_name: Arc::from(config.server_name()),
            net_tx,
            cmd_rx,
            sessions: HashMap::new(),
            next_id: 1,
        }
    }

    // 只做收包路由与下行分发 协议状态全在各连接任务里
    pub async fn run(mut self) -> Result<()> {
        let socket = Arc::clone(&self.socket);
        let mut buf = vec![0u8; MAX_DATAGRAM];
        // 任务退出通知 由 manager 移除路由 不会误删已经接管的新会话
        let (done_tx, mut done_rx) = mpsc::channel::<(u64, SocketAddr)>(64);

        loop {
            tokio::select! {
                received = socket.recv_from(&mut buf) => {
                    let (len, peer) = received?;
                    let packet = &buf[..len];
                    if packet.is_empty() {
                        continue;
                    }

                    // 离线 ping 无会话 直接在收包循环回送
                    if matches!(packet[0], ID_UNCONNECTED_PING | ID_UNCONNECTED_PING_OPEN_CONNECTIONS) {
                        if let Ok(ping) = UnconnectedPing::decode(packet) {
                            let pong = UnconnectedPong::reply_to(ping, self.server_id, self.server_name.as_ref());
                            if let Ok(data) = pong.encode() {
                                let _ = socket.send_to(&data, peer).await;
                            }
                        }
                        continue;
                    }

                    // 握手首包一律开新会话 同端点旧会话立刻退役
                    // 新任务马上接管 旧任务在后台收尾 它的 Closed 带旧 id 不会影响新玩家
                    if packet[0] == ID_OPEN_CONNECTION_REQUEST_1 {
                        if let Some(old) = self.sessions.remove(&peer) {
                            let _ = old.tx.try_send(ConnCmd::Shutdown);
                        }
                        let id = self.next_id;
                        self.next_id += 1;
                        let (tx, rx) = mpsc::channel(256);
                        self.sessions.insert(peer, ConnHandle { id, tx, ready: false });
                        spawn_connection(
                            id,
                            peer,
                            self.server_id,
                            Arc::clone(&socket),
                            self.net_tx.clone(),
                            done_tx.clone(),
                            rx,
                            Bytes::copy_from_slice(packet),
                        );
                        continue;
                    }

                    if let Some(handle) = self.sessions.get(&peer) {
                        let tx = handle.tx.clone();
                        // 上行也不能丢 ACK 丢了会触发大量重传 下行翻倍后雪崩
                        // 缓冲满时阻塞一下 由连接任务的消费速度自然背压
                        if let Err(mpsc::error::TrySendError::Full(cmd)) =
                            tx.try_send(ConnCmd::Datagram(Bytes::copy_from_slice(packet)))
                        {
                            let _ = tx.send(cmd).await;
                        }
                    }
                }
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        SendCmd::Console(_) => {}
                        // 登录完成 之后的下行才允许发到这条连接
                        SendCmd::SessionReady { addr } => {
                            match self.sessions.get_mut(&addr) {
                                Some(handle) => {
                                    handle.ready = true;
                                }
                                None => warn!("[{addr}] session ready but no route"),
                            }
                        }
                        SendCmd::GamePackets { addr, bodies } => {
                            if let Some(handle) = self.sessions.get(&addr).filter(|h| h.ready) {
                                let tx = handle.tx.clone();
                                // 下行绝不能丢 区块包丢了客户端会一直卡在加载界面
                                // 缓冲满时阻塞等待 由连接任务的发送速度自然背压
                                if let Err(mpsc::error::TrySendError::Full(cmd)) =
                                    tx.try_send(ConnCmd::Downlink { bodies })
                                {
                                    let _ = tx.send(cmd).await;
                                }
                            }
                        }
                    }
                }
                Some((id, addr)) = done_rx.recv() => {
                    // 只有路由还是这个 id 才移除 新连接接管后旧任务退出不会误删
                    if self.sessions.get(&addr).is_some_and(|handle| handle.id == id) {
                        self.sessions.remove(&addr);
                    }
                }
                else => break,
            }
        }
        Ok(())
    }
}

// 每条连接一个任务 握手 重传 超时判定都独立跑
// 旧会话退役和新会话建立互不阻塞 退出时保证 Closed 事件只发一次
#[allow(clippy::too_many_arguments)]
fn spawn_connection(
    id: u64,
    addr: SocketAddr,
    server_id: u64,
    socket: Arc<UdpSocket>,
    net_tx: mpsc::Sender<NetEvent>,
    done_tx: mpsc::Sender<(u64, SocketAddr)>,
    mut rx: mpsc::Receiver<ConnCmd>,
    first: Bytes,
) {
    tokio::spawn(async move {
        let mut session = Session::new(id, addr, server_id);
        let mut ticker = interval(Duration::from_millis(SESSION_TICK_MS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut pending = Some(first);
        let mut finished = false;
        let mut sent_closed = false;
        let mut reason = String::from("closed");

        while !finished {
            let mut out = Vec::new();
            let mut events = Vec::new();

            match pending.take() {
                Some(packet) => {
                    let _ = session.handle_datagram(&packet, &mut out, &mut events);
                }
                None => {
                    // 队列里已排队的命令一次性取空 批量处理后再统一发送
                    // 一次只处理一条会让区块下发的突发流量堆积并丢包
                    let mut cmds = Vec::new();
                    tokio::select! {
                        cmd = rx.recv() => match cmd {
                            Some(cmd) => {
                                cmds.push(cmd);
                                while let Ok(cmd) = rx.try_recv() {
                                    cmds.push(cmd);
                                }
                            }
                            None => {
                                reason = "replaced".into();
                                finished = true;
                            }
                        },
                        _ = ticker.tick() => {
                            session.tick(Instant::now(), &mut out, &mut events);
                        }
                    }
                    for cmd in cmds {
                        match cmd {
                            ConnCmd::Datagram(packet) => {
                                let _ = session.handle_datagram(&packet, &mut out, &mut events);
                            }
                            ConnCmd::Downlink { bodies } => {
                                for body in bodies {
                                    let _ = session.send_game(body, &mut out);
                                }
                            }
                            ConnCmd::Shutdown => {
                                reason = "replaced".into();
                                finished = true;
                            }
                        }
                    }
                }
            }

            for data in out {
                let _ = socket.send_to(&data, addr).await;
            }

            for event in events {
                let (net, closed) = match event {
                    SessionEvent::Opened { id, addr, client_id } => (
                        NetEvent::SessionOpened { id, addr, client_id },
                        false,
                    ),
                    SessionEvent::Closed { id, addr, reason } => (
                        NetEvent::SessionClosed { id, addr, reason },
                        true,
                    ),
                    SessionEvent::Game { id, addr, payload } => (
                        NetEvent::GamePacket { id, addr, payload },
                        false,
                    ),
                };
                if closed {
                    finished = true;
                    sent_closed = true;
                }
                if net_tx.send(net).await.is_err() {
                    finished = true;
                }
            }

            // 客户端主动断开(0x15)时 Closed 事件已经产生 这里兜住其余路径
            if session.is_closed() {
                finished = true;
            }
        }
        if !sent_closed {
            let _ = net_tx
                .send(NetEvent::SessionClosed { id, addr, reason })
                .await;
        }
        let _ = done_tx.send((id, addr)).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raknet::MAGIC;
    use crate::raknet::protocol::packets::{
        ID_OPEN_CONNECTION_REPLY_1, ID_OPEN_CONNECTION_REQUEST_1, ID_UNCONNECTED_PONG,
    };
    use std::time::Duration;

    fn ping_packet(pid: u8, ping_id: u64) -> Vec<u8> {
        let mut p = vec![pid];
        p.extend_from_slice(&ping_id.to_be_bytes());
        p.extend_from_slice(&MAGIC);
        p
    }

    async fn start() -> (
        SocketAddr,
        mpsc::Receiver<NetEvent>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let (tx, rx) = mpsc::channel(16);
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let server = RakNetServer::new(socket, &Config::default(), tx, 0x006d_6f6a_6f72, cmd_rx);
        // 把下行发送端留在 task 里 否则通道关闭会让服务器立即退出
        let handle = tokio::spawn(async move {
            let _keep_cmd_tx = cmd_tx;
            server.run().await
        });
        (addr, rx, handle)
    }

    #[tokio::test]
    async fn ping_gets_pong_for_both_ids() {
        let (addr, _rx, handle) = start().await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ping_id = 0x0102_0304_0506_0708u64;

        for pid in [ID_UNCONNECTED_PING, ID_UNCONNECTED_PING_OPEN_CONNECTIONS] {
            client
                .send_to(&ping_packet(pid, ping_id), addr)
                .await
                .unwrap();
            let mut buf = [0u8; 2048];
            let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buf))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(buf[0], ID_UNCONNECTED_PONG);
            assert_eq!(&buf[1..9], &ping_id.to_be_bytes()[..]);
            assert_eq!(&buf[17..33], MAGIC.as_slice());
            assert!(len > 35);
        }
        handle.abort();
    }

    #[tokio::test]
    async fn connection_request_1_gets_reply_1() {
        let (addr, _rx, handle) = start().await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let mut request = vec![ID_OPEN_CONNECTION_REQUEST_1];
        request.extend_from_slice(&MAGIC);
        request.push(6);
        request.resize(1492, 0);
        client.send_to(&request, addr).await.unwrap();

        let mut buf = [0u8; 2048];
        let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(buf[0], ID_OPEN_CONNECTION_REPLY_1);
        assert_eq!(&buf[1..17], MAGIC.as_slice());
        assert_eq!(len, 1 + 16 + 8 + 1 + 2);
        assert_eq!(u16::from_be_bytes([buf[len - 2], buf[len - 1]]), 1464);
        handle.abort();
    }

    // 同一端口重复握手 客户端退出后立刻重连也必须有回应 不能等会话超时
    #[tokio::test]
    async fn reconnect_from_same_port_gets_reply_again() {
        let (addr, mut rx, handle) = start().await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let mut request = vec![ID_OPEN_CONNECTION_REQUEST_1];
        request.extend_from_slice(&MAGIC);
        request.push(6);
        request.resize(1492, 0);

        for _ in 0..2 {
            client.send_to(&request, addr).await.unwrap();
            let mut buf = [0u8; 2048];
            let (len, _) = tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buf))
                .await
                .expect("同端点重连必须立刻回 Reply1")
                .unwrap();
            assert_eq!(buf[0], ID_OPEN_CONNECTION_REPLY_1);
            assert!(len > 0);
        }

        // 旧会话退役 上层必须收到带旧 id 的关闭事件 用于区分新旧连接
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("应收到旧会话的关闭事件")
            .expect("事件通道未关闭");
        match event {
            NetEvent::SessionClosed { id, .. } => {
                assert_eq!(id, 1, "旧会话 id 应为 1");
            }
            other => panic!("期望 SessionClosed 实际 {other:?}"),
        }
        handle.abort();
    }

    #[tokio::test]
    async fn malformed_packets_do_not_stop_listener() {
        let (addr, _rx, handle) = start().await;
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        client.send_to(&[0x01, 0x00], addr).await.unwrap();
        let mut bad = ping_packet(ID_UNCONNECTED_PING, 9);
        bad[10] = 0x00;
        client.send_to(&bad, addr).await.unwrap();

        let ping_id = 42u64;
        client
            .send_to(&ping_packet(ID_UNCONNECTED_PING, ping_id), addr)
            .await
            .unwrap();
        let mut buf = [0u8; 2048];
        tokio::time::timeout(Duration::from_secs(2), client.recv_from(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(buf[0], ID_UNCONNECTED_PONG);
        assert_eq!(&buf[1..9], &ping_id.to_be_bytes()[..]);
        handle.abort();
    }
}
