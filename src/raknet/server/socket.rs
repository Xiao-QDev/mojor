use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{BufMut, Bytes, BytesMut};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{trace, warn};

use crate::config::Config;
use crate::error::Result;
use crate::network::SendCmd;
use crate::raknet::GAME_PACKET_PREFIX;
use crate::raknet::protocol::packets::{
    ID_UNCONNECTED_PING, ID_UNCONNECTED_PING_OPEN_CONNECTIONS, UnconnectedPing, UnconnectedPong,
};
use crate::raknet::server::session::{Session, SessionEvent};
use crate::utils::binary::hexdump;

pub const MAX_DATAGRAM: usize = u16::MAX as usize;
pub const SESSION_TICK_MS: u64 = 50;

#[derive(Debug)]
pub enum NetEvent {
    SessionOpened { addr: SocketAddr, client_id: u64 },
    SessionClosed { addr: SocketAddr, reason: String },
    GamePacket { addr: SocketAddr, payload: Bytes },
}

pub struct RakNetServer {
    socket: Arc<UdpSocket>,
    server_id: u64,
    server_name: Arc<str>,
    net_tx: mpsc::Sender<NetEvent>,
    cmd_rx: mpsc::Receiver<SendCmd>,
    sessions: HashMap<SocketAddr, Session>,
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
        }
    }

    pub async fn run(mut self) -> Result<()> {
        let socket = Arc::clone(&self.socket);
        let mut buf = vec![0u8; MAX_DATAGRAM];
        let mut ticker = interval(Duration::from_millis(SESSION_TICK_MS));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut dropped: u64 = 0;

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
                        match UnconnectedPing::decode(packet) {
                            Ok(ping) => {
                                let pong = UnconnectedPong::reply_to(ping, self.server_id, self.server_name.as_ref());
                                match pong.encode() {
                                    Ok(data) => {
                                        if let Err(e) = socket.send_to(&data, peer).await {
                                            warn!(%peer, error = %e, "pong 发送失败");
                                        }
                                    }
                                    Err(e) => warn!(%peer, error = %e, "pong 编码失败"),
                                }
                            }
                            Err(e) => warn!(%peer, error = %e, "ping 解码失败\n{}", hexdump(packet)),
                        }
                        continue;
                    }

                    let mut out = Vec::new();
                    let mut events = Vec::new();
                    {
                        let session = self
                            .sessions
                            .entry(peer)
                            .or_insert_with(|| Session::new(peer, self.server_id));
                        if let Err(e) = session.handle_datagram(packet, &mut out, &mut events) {
                            warn!(%peer, error = %e, "处理数据包失败\n{}", hexdump(packet));
                        }
                    }

                    for data in out {
                        if let Err(e) = socket.send_to(&data, peer).await {
                            trace!(%peer, error = %e, "发送失败");
                        }
                    }
                    self.forward(events, &mut dropped);
                }
                command = self.cmd_rx.recv() => {
                    match command {
                        Some(SendCmd::GamePackets { addr, bodies }) => {
                            if let Some(session) = self.sessions.get_mut(&addr) {
                                for body in bodies {
                                    // 0x8e 前缀在这里拼 只做一次拷贝
                                    let mut payload = BytesMut::with_capacity(1 + body.len());
                                    payload.put_u8(GAME_PACKET_PREFIX);
                                    payload.put_slice(&body);
                                    let mut out = Vec::new();
                                    if let Err(e) = session.send_game(payload.freeze(), &mut out) {
                                        warn!(%addr, error = %e, "下行编码失败");
                                    }
                                    for data in out {
                                        if let Err(e) = socket.send_to(&data, addr).await {
                                            trace!(%addr, error = %e, "发送失败");
                                        }
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = ticker.tick() => {
                    let now = Instant::now();
                    let mut outgoing: Vec<(SocketAddr, Bytes)> = Vec::new();
                    let mut events = Vec::new();
                    let mut closed = Vec::new();

                    for (addr, session) in &mut self.sessions {
                        let mut out = Vec::new();
                        let mut session_events = Vec::new();
                        session.tick(now, &mut out, &mut session_events);

                        for data in out {
                            outgoing.push((*addr, data));
                        }
                        if session_events
                            .iter()
                            .any(|e| matches!(e, SessionEvent::Closed { .. }))
                        {
                            closed.push(*addr);
                        }
                        events.extend(session_events);
                    }

                    for (addr, data) in outgoing {
                        if let Err(e) = socket.send_to(&data, addr).await {
                            trace!(%addr, error = %e, "发送失败");
                        }
                    }
                    for addr in closed {
                        self.sessions.remove(&addr);
                    }
                    self.forward(events, &mut dropped);
                }
            }
        }
        Ok(())
    }

    fn forward(&self, events: Vec<SessionEvent>, dropped: &mut u64) {
        for event in events {
            let net_event = match event {
                SessionEvent::Opened { addr, client_id } => {
                    NetEvent::SessionOpened { addr, client_id }
                }
                SessionEvent::Closed { addr, reason } => NetEvent::SessionClosed { addr, reason },
                SessionEvent::Game { addr, payload } => NetEvent::GamePacket { addr, payload },
            };

            if self.net_tx.try_send(net_event).is_err() {
                *dropped += 1;
                if (*dropped).is_multiple_of(100) {
                    warn!(dropped = *dropped, "网络事件队列已满 丢弃事件");
                }
            }
        }
    }
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
