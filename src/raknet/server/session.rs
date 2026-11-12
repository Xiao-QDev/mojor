use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tracing::trace;

use crate::error::Result;
use crate::raknet::protocol::frame::{
    Ack, DataFrame, EncapsulatedPacket, ID_ACK, ID_DATA_PACKET_BASE, ID_NACK, RELIABILITY_RELIABLE,
    RELIABILITY_RELIABLE_ORDERED, SplitInfo,
};
use crate::raknet::protocol::packets::{
    ClientConnect, ClientHandshake, ID_CLIENT_CONNECT, ID_CLIENT_DISCONNECT, ID_CLIENT_HANDSHAKE,
    ID_OPEN_CONNECTION_REQUEST_1, ID_OPEN_CONNECTION_REQUEST_2, ID_PING, MAX_MTU, MIN_MTU,
    OpenConnectionReply1, OpenConnectionReply2, OpenConnectionRequest1, OpenConnectionRequest2,
    Ping, Pong, ServerHandshake,
};

pub const WINDOW_SIZE: u32 = 2048;
pub const MAX_SPLIT_SIZE: u32 = 128;
pub const MAX_SPLIT_COUNT: usize = 4;
pub const SESSION_TIMEOUT: Duration = Duration::from_secs(10);
pub const RETRY_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Unconnected,
    Connecting1,
    Connecting2,
    Connected,
}

#[derive(Debug)]
pub enum SessionEvent {
    Opened { addr: SocketAddr, client_id: u64 },
    Closed { addr: SocketAddr, reason: String },
    Game { addr: SocketAddr, payload: Bytes },
}

struct SentFrame {
    frame: DataFrame,
    sent_at: Instant,
}

struct SplitAssembler {
    count: u32,
    parts: Vec<Option<Bytes>>,
}

impl SplitAssembler {
    fn new(count: u32) -> Self {
        Self {
            count,
            parts: vec![None; count as usize],
        }
    }

    fn insert(&mut self, index: u32, data: Bytes) {
        if let Some(slot) = self.parts.get_mut(index as usize) {
            *slot = Some(data);
        }
    }

    fn complete(&self) -> bool {
        self.parts.iter().all(Option::is_some)
    }

    fn assemble(&mut self) -> Bytes {
        let mut out = Vec::new();
        for part in &mut self.parts {
            if let Some(data) = part.take() {
                out.extend_from_slice(&data);
            }
        }
        Bytes::from(out)
    }
}

pub struct Session {
    pub addr: SocketAddr,
    pub server_id: u64,
    pub state: SessionState,
    pub mtu: u16,
    pub client_id: u64,

    last_update: Instant,
    active: bool,

    send_seq: u32,
    message_index: u32,
    channel_index: [u32; 32],
    split_id: u16,
    send_queue: Vec<EncapsulatedPacket>,
    recovery: BTreeMap<u32, SentFrame>,

    last_seq: i64,
    window_start: u32,
    window_end: u32,
    received: BTreeSet<u32>,
    ack_queue: BTreeSet<u32>,
    nack_queue: BTreeSet<u32>,

    reliable_window: BTreeMap<u32, EncapsulatedPacket>,
    last_reliable: i64,
    reliable_start: u32,
    reliable_end: u32,

    splits: HashMap<u16, SplitAssembler>,
}

impl Session {
    pub fn new(addr: SocketAddr, server_id: u64) -> Self {
        Self {
            addr,
            server_id,
            state: SessionState::Unconnected,
            mtu: MIN_MTU,
            client_id: 0,
            last_update: Instant::now(),
            active: false,
            send_seq: 0,
            message_index: 0,
            channel_index: [0; 32],
            split_id: 0,
            send_queue: Vec::new(),
            recovery: BTreeMap::new(),
            last_seq: -1,
            window_start: 0,
            window_end: WINDOW_SIZE,
            received: BTreeSet::new(),
            ack_queue: BTreeSet::new(),
            nack_queue: BTreeSet::new(),
            reliable_window: BTreeMap::new(),
            last_reliable: -1,
            reliable_start: 0,
            reliable_end: WINDOW_SIZE,
            splits: HashMap::new(),
        }
    }

    pub fn handle_datagram(
        &mut self,
        buf: &[u8],
        out: &mut Vec<Bytes>,
        events: &mut Vec<SessionEvent>,
    ) -> Result<()> {
        self.last_update = Instant::now();
        self.active = true;

        if buf.is_empty() {
            return Ok(());
        }

        match buf[0] {
            ID_OPEN_CONNECTION_REQUEST_1 => self.on_open_connection_request_1(buf, out)?,
            ID_OPEN_CONNECTION_REQUEST_2 => self.on_open_connection_request_2(buf, out)?,
            ID_ACK => {
                let ack = Ack::decode(buf)?;
                for seq in ack.sequences {
                    self.recovery.remove(&seq);
                }
            }
            ID_NACK => self.on_nack(buf, out)?,
            pid if (ID_DATA_PACKET_BASE..=0x8f).contains(&pid) => {
                self.on_data_frame(buf, out, events)?
            }
            pid => trace!(pid, "忽略未知数据包"),
        }

        self.flush_send_queue(out);
        Ok(())
    }

    pub fn tick(&mut self, now: Instant, out: &mut Vec<Bytes>, events: &mut Vec<SessionEvent>) {
        if !self.active && now.duration_since(self.last_update) > SESSION_TIMEOUT {
            events.push(SessionEvent::Closed {
                addr: self.addr,
                reason: "timeout".into(),
            });
            return;
        }
        self.active = false;

        if !self.ack_queue.is_empty() {
            let ack = Ack {
                sequences: self.ack_queue.iter().copied().collect(),
            };
            out.push(ack.encode(ID_ACK));
            self.ack_queue.clear();
        }
        if !self.nack_queue.is_empty() {
            let nack = Ack {
                sequences: self.nack_queue.iter().copied().collect(),
            };
            out.push(nack.encode(ID_NACK));
            self.nack_queue.clear();
        }

        // 只保留仍在窗口内的接收记录
        self.received = self.received.split_off(&self.window_start);

        if let Some(deadline) = now.checked_sub(RETRY_TIMEOUT) {
            let stale: Vec<u32> = self
                .recovery
                .iter()
                .filter(|(_, sent)| sent.sent_at < deadline)
                .map(|(seq, _)| *seq)
                .collect();
            for seq in stale {
                if let Some(mut sent) = self.recovery.remove(&seq) {
                    let new_seq = self.next_send_seq();
                    sent.frame.seq = new_seq;
                    sent.sent_at = now;
                    let data = sent.frame.encode();
                    self.recovery.insert(new_seq, sent);
                    out.push(data);
                }
            }
        }

        self.flush_send_queue(out);
    }

    pub fn send_game(&mut self, payload: Bytes, out: &mut Vec<Bytes>) -> Result<()> {
        let pk = EncapsulatedPacket {
            reliability: RELIABILITY_RELIABLE_ORDERED,
            message_index: None,
            order_index: None,
            order_channel: 0,
            split: None,
            payload,
        };
        self.add_to_queue(pk, out)?;
        self.flush_send_queue(out);
        Ok(())
    }

    #[cfg(test)]
    fn recovery_len(&self) -> usize {
        self.recovery.len()
    }

    fn on_open_connection_request_1(&mut self, buf: &[u8], out: &mut Vec<Bytes>) -> Result<()> {
        if self.state != SessionState::Unconnected {
            return Ok(());
        }
        let request = OpenConnectionRequest1::decode(buf)?;
        self.mtu = request.mtu.clamp(MIN_MTU, MAX_MTU);
        let reply = OpenConnectionReply1 {
            server_id: self.server_id,
            mtu: self.mtu,
        };
        out.push(reply.encode());
        self.state = SessionState::Connecting1;
        Ok(())
    }

    fn on_open_connection_request_2(&mut self, buf: &[u8], out: &mut Vec<Bytes>) -> Result<()> {
        if self.state != SessionState::Connecting1 {
            return Ok(());
        }
        let request = OpenConnectionRequest2::decode(buf)?;
        self.client_id = request.client_id;
        self.mtu = request.mtu.clamp(MIN_MTU, MAX_MTU);
        let reply = OpenConnectionReply2 {
            server_id: self.server_id,
            client: self.addr,
            mtu: self.mtu,
        };
        out.push(reply.encode());
        self.state = SessionState::Connecting2;
        Ok(())
    }

    fn on_data_frame(
        &mut self,
        buf: &[u8],
        out: &mut Vec<Bytes>,
        events: &mut Vec<SessionEvent>,
    ) -> Result<()> {
        if !matches!(
            self.state,
            SessionState::Connecting2 | SessionState::Connected
        ) {
            return Ok(());
        }

        let frame = DataFrame::decode(buf)?;

        if frame.seq < self.window_start
            || frame.seq > self.window_end
            || self.received.contains(&frame.seq)
        {
            return Ok(());
        }

        self.nack_queue.remove(&frame.seq);
        self.ack_queue.insert(frame.seq);
        self.received.insert(frame.seq);

        let diff = frame.seq as i64 - self.last_seq;
        if diff != 1 {
            let start = (self.last_seq + 1).max(0) as u32;
            for seq in (start..frame.seq).take(512) {
                if !self.received.contains(&seq) {
                    self.nack_queue.insert(seq);
                }
            }
        }
        if diff >= 1 {
            self.last_seq = frame.seq as i64;
            self.window_start = self.window_start.saturating_add(diff as u32);
            self.window_end = self.window_end.saturating_add(diff as u32);
        }

        for packet in frame.packets {
            self.on_encapsulated(packet, out, events)?;
        }
        Ok(())
    }

    fn on_nack(&mut self, buf: &[u8], out: &mut Vec<Bytes>) -> Result<()> {
        let nack = Ack::decode(buf)?;
        for seq in nack.sequences {
            if let Some(mut sent) = self.recovery.remove(&seq) {
                let new_seq = self.next_send_seq();
                sent.frame.seq = new_seq;
                sent.sent_at = Instant::now();
                let data = sent.frame.encode();
                self.recovery.insert(new_seq, sent);
                out.push(data);
            }
        }
        Ok(())
    }

    fn on_encapsulated(
        &mut self,
        packet: EncapsulatedPacket,
        out: &mut Vec<Bytes>,
        events: &mut Vec<SessionEvent>,
    ) -> Result<()> {
        let Some(message_index) = packet.message_index else {
            return self.route_or_split(packet, out, events);
        };

        if message_index < self.reliable_start || message_index > self.reliable_end {
            return Ok(());
        }

        if message_index as i64 - self.last_reliable == 1 {
            self.advance_reliable();
            self.route_or_split(packet, out, events)?;

            loop {
                let next = (self.last_reliable + 1) as u32;
                let Some(next_packet) = self.reliable_window.remove(&next) else {
                    break;
                };
                self.advance_reliable();
                self.route_or_split(next_packet, out, events)?;
            }
        } else {
            self.reliable_window.insert(message_index, packet);
        }
        Ok(())
    }

    fn advance_reliable(&mut self) {
        self.last_reliable += 1;
        self.reliable_start = self.reliable_start.saturating_add(1);
        self.reliable_end = self.reliable_end.saturating_add(1);
    }

    fn route_or_split(
        &mut self,
        packet: EncapsulatedPacket,
        out: &mut Vec<Bytes>,
        events: &mut Vec<SessionEvent>,
    ) -> Result<()> {
        let Some(split) = packet.split else {
            return self.route(packet.payload, out, events);
        };

        if split.count == 0 || split.count > MAX_SPLIT_SIZE || split.index >= split.count {
            return Ok(());
        }

        if !self.splits.contains_key(&split.id)
            && self.splits.len() >= MAX_SPLIT_COUNT
            && let Some(oldest) = self.splits.keys().next().copied()
        {
            self.splits.remove(&oldest);
        }

        let entry = self
            .splits
            .entry(split.id)
            .or_insert_with(|| SplitAssembler::new(split.count));
        if entry.count != split.count {
            return Ok(());
        }
        entry.insert(split.index, packet.payload);

        if entry.complete() {
            let data = entry.assemble();
            self.splits.remove(&split.id);
            trace!(client = %self.addr, split_id = split.id, count = split.count, "分片重组完成");
            self.route(data, out, events)?;
        }
        Ok(())
    }

    fn route(
        &mut self,
        payload: Bytes,
        out: &mut Vec<Bytes>,
        events: &mut Vec<SessionEvent>,
    ) -> Result<()> {
        if payload.is_empty() {
            return Ok(());
        }

        let id = payload[0];
        if id < 0x80 {
            match id {
                ID_CLIENT_CONNECT if self.state == SessionState::Connecting2 => {
                    let packet = ClientConnect::decode(&payload)?;
                    self.client_id = packet.client_id;
                    // 与 Nukkit 一致 这里填对端地址
                    let handshake = ServerHandshake {
                        server: self.addr,
                        send_ping: packet.send_ping,
                        send_pong: packet.send_ping.wrapping_add(1000),
                    };
                    self.add_to_queue(EncapsulatedPacket::unreliable(handshake.encode()), out)?;
                }
                ID_CLIENT_HANDSHAKE if self.state == SessionState::Connecting2 => {
                    ClientHandshake::decode(&payload)?;
                    self.state = SessionState::Connected;
                    events.push(SessionEvent::Opened {
                        addr: self.addr,
                        client_id: self.client_id,
                    });
                }
                ID_CLIENT_DISCONNECT => {
                    events.push(SessionEvent::Closed {
                        addr: self.addr,
                        reason: "client disconnect".into(),
                    });
                }
                ID_PING => {
                    let ping = Ping::decode(&payload)?;
                    let pong = Pong {
                        ping_id: ping.ping_id,
                    };
                    self.add_to_queue(EncapsulatedPacket::unreliable(pong.encode()), out)?;
                }
                _ => trace!(id, "未处理的内部包"),
            }
        } else if self.state == SessionState::Connected {
            events.push(SessionEvent::Game {
                addr: self.addr,
                payload,
            });
        }
        Ok(())
    }

    fn add_to_queue(&mut self, mut packet: EncapsulatedPacket, out: &mut Vec<Bytes>) -> Result<()> {
        if matches!(
            packet.reliability,
            RELIABILITY_RELIABLE | RELIABILITY_RELIABLE_ORDERED
        ) && packet.message_index.is_none()
        {
            packet.message_index = Some(self.next_message_index());
        }
        if packet.reliability == RELIABILITY_RELIABLE_ORDERED && packet.order_index.is_none() {
            let channel = packet.order_channel as usize;
            // RakNet 的有序号从 0 开始 客户端按 orderedReadIndex 严格递增放行
            // 首个包必须是 0 否则后续全被扣在有序缓冲里
            packet.order_index = Some(self.channel_index[channel]);
            self.channel_index[channel] = self.channel_index[channel].wrapping_add(1);
        }

        if packet.encoded_len() + 4 > self.mtu as usize {
            let max = (self.mtu as usize).saturating_sub(34).max(64);
            self.split_id = self.split_id.wrapping_add(1);
            let split_id = self.split_id;
            let total = packet.payload.len().div_ceil(max) as u32;

            for (index, chunk) in packet.payload.chunks(max).enumerate() {
                let mut part = EncapsulatedPacket {
                    reliability: packet.reliability,
                    message_index: None,
                    order_index: packet.order_index,
                    order_channel: packet.order_channel,
                    split: Some(SplitInfo {
                        count: total,
                        id: split_id,
                        index: index as u32,
                    }),
                    payload: Bytes::copy_from_slice(chunk),
                };
                if matches!(
                    part.reliability,
                    RELIABILITY_RELIABLE | RELIABILITY_RELIABLE_ORDERED
                ) {
                    part.message_index = Some(self.next_message_index());
                }
                self.push_to_queue(part, out);
            }
        } else {
            self.push_to_queue(packet, out);
        }
        Ok(())
    }

    fn push_to_queue(&mut self, packet: EncapsulatedPacket, out: &mut Vec<Bytes>) {
        let current = 4 + self
            .send_queue
            .iter()
            .map(EncapsulatedPacket::encoded_len)
            .sum::<usize>();
        if !self.send_queue.is_empty() && current + packet.encoded_len() > self.mtu as usize {
            self.flush_send_queue(out);
        }
        self.send_queue.push(packet);
    }

    fn flush_send_queue(&mut self, out: &mut Vec<Bytes>) {
        if self.send_queue.is_empty() {
            return;
        }
        let packets = std::mem::take(&mut self.send_queue);
        let seq = self.next_send_seq();
        let frame = DataFrame::new(seq, packets);
        let data = frame.encode();
        self.recovery.insert(
            seq,
            SentFrame {
                frame,
                sent_at: Instant::now(),
            },
        );
        out.push(data);
    }

    fn next_send_seq(&mut self) -> u32 {
        let seq = self.send_seq;
        self.send_seq = self.send_seq.wrapping_add(1);
        seq
    }

    fn next_message_index(&mut self) -> u32 {
        let index = self.message_index;
        self.message_index = self.message_index.wrapping_add(1);
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raknet::MAGIC;
    use crate::raknet::protocol::packets::ID_OPEN_CONNECTION_REQUEST_1 as REQ1;

    fn peer() -> SocketAddr {
        "127.0.0.1:50000".parse().unwrap()
    }

    fn request_1(mtu: usize) -> Vec<u8> {
        let mut buf = vec![REQ1];
        buf.extend_from_slice(&MAGIC);
        buf.push(6);
        buf.resize(mtu, 0);
        buf
    }

    fn request_2(mtu: u16, client_id: u64) -> Vec<u8> {
        let mut w = crate::utils::binary::BinaryWriter::new();
        w.put_u8(ID_OPEN_CONNECTION_REQUEST_2);
        w.put_bytes(&MAGIC);
        w.put_u8(4);
        for _ in 0..4 {
            w.put_u8(0xff);
        }
        w.put_u16(19132);
        w.put_u16(mtu);
        w.put_u64(client_id);
        w.freeze().to_vec()
    }

    fn data_frame(seq: u32, packets: Vec<EncapsulatedPacket>) -> Vec<u8> {
        DataFrame::new(seq, packets).encode().to_vec()
    }

    fn client_connect(client_id: u64, send_ping: u64) -> Vec<u8> {
        let mut w = crate::utils::binary::BinaryWriter::new();
        w.put_u8(ID_CLIENT_CONNECT);
        w.put_u64(client_id);
        w.put_u64(send_ping);
        w.put_u8(0);
        w.freeze().to_vec()
    }

    fn client_handshake() -> Vec<u8> {
        let mut w = crate::utils::binary::BinaryWriter::new();
        w.put_u8(ID_CLIENT_HANDSHAKE);
        w.put_u8(4);
        for _ in 0..4 {
            w.put_u8(0xff);
        }
        w.put_u16(50000);
        for _ in 0..10 {
            w.put_u8(4);
            for _ in 0..4 {
                w.put_u8(0xff);
            }
            w.put_u16(0);
        }
        w.put_u64(111);
        w.put_u64(222);
        w.freeze().to_vec()
    }

    fn connected_session() -> Session {
        let mut session = Session::new(peer(), 42);
        let mut out = Vec::new();
        let mut events = Vec::new();
        session
            .handle_datagram(&request_1(1492), &mut out, &mut events)
            .unwrap();
        session
            .handle_datagram(&request_2(1200, 7), &mut out, &mut events)
            .unwrap();
        session
            .handle_datagram(
                &data_frame(
                    0,
                    vec![EncapsulatedPacket::unreliable(client_connect(7, 99))],
                ),
                &mut out,
                &mut events,
            )
            .unwrap();
        session
            .handle_datagram(
                &data_frame(1, vec![EncapsulatedPacket::reliable(client_handshake(), 0)]),
                &mut out,
                &mut events,
            )
            .unwrap();
        session
    }

    // 有序包的 order_index 必须从 0 开始 否则客户端有序缓冲永远放行不了
    #[test]
    fn game_packets_start_order_index_at_zero() {
        let mut session = connected_session();
        let mut out = Vec::new();
        session
            .send_game(Bytes::from_static(b"\x8e\x90"), &mut out)
            .unwrap();
        session
            .send_game(Bytes::from_static(b"\x8e\x95"), &mut out)
            .unwrap();

        let orders: Vec<u32> = out
            .iter()
            .map(|data| {
                let frame = DataFrame::decode(data).unwrap();
                frame.packets[0].order_index.unwrap()
            })
            .collect();
        assert_eq!(orders, vec![0, 1]);
    }

    #[test]
    fn unconnected_handshake_completes() {
        let mut session = Session::new(peer(), 42);
        let mut out = Vec::new();
        let mut events = Vec::new();

        session
            .handle_datagram(&request_1(1492), &mut out, &mut events)
            .unwrap();
        assert_eq!(session.state, SessionState::Connecting1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], 0x06);

        out.clear();
        session
            .handle_datagram(&request_2(1200, 0x1122), &mut out, &mut events)
            .unwrap();
        assert_eq!(session.state, SessionState::Connecting2);
        assert_eq!(session.mtu, 1200);
        assert_eq!(session.client_id, 0x1122);
        assert_eq!(out[0][0], 0x08);
    }

    #[test]
    fn client_connect_gets_server_handshake_and_then_opens() {
        let mut session = Session::new(peer(), 42);
        let mut out = Vec::new();
        let mut events = Vec::new();

        session
            .handle_datagram(&request_1(1492), &mut out, &mut events)
            .unwrap();
        session
            .handle_datagram(&request_2(1200, 7), &mut out, &mut events)
            .unwrap();
        out.clear();

        session
            .handle_datagram(
                &data_frame(
                    0,
                    vec![EncapsulatedPacket::unreliable(client_connect(7, 99))],
                ),
                &mut out,
                &mut events,
            )
            .unwrap();
        assert_eq!(session.state, SessionState::Connecting2);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], 0x84);

        let frame = DataFrame::decode(&out[0]).unwrap();
        assert_eq!(frame.packets.len(), 1);
        assert_eq!(frame.packets[0].payload[0], 0x10);

        out.clear();
        session
            .handle_datagram(
                &data_frame(1, vec![EncapsulatedPacket::reliable(client_handshake(), 0)]),
                &mut out,
                &mut events,
            )
            .unwrap();
        assert_eq!(session.state, SessionState::Connected);
        assert!(matches!(
            events.first(),
            Some(SessionEvent::Opened { client_id: 7, .. })
        ));
    }

    #[test]
    fn reliable_packets_are_ordered_and_deduplicated() {
        let mut session = connected_session();
        let mut out = Vec::new();
        let mut events = Vec::new();

        // connected_session 已消费 messageIndex 0 先到 2 再补 1 应按 1,2 顺序投递
        session
            .handle_datagram(
                &data_frame(
                    2,
                    vec![EncapsulatedPacket::reliable(
                        Bytes::from_static(b"\x8e\x02"),
                        2,
                    )],
                ),
                &mut out,
                &mut events,
            )
            .unwrap();
        assert!(events.is_empty());

        session
            .handle_datagram(
                &data_frame(
                    3,
                    vec![EncapsulatedPacket::reliable(
                        Bytes::from_static(b"\x8e\x01"),
                        1,
                    )],
                ),
                &mut out,
                &mut events,
            )
            .unwrap();
        let payloads: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Game { payload, .. } => Some(payload.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(payloads.len(), 2);
        assert_eq!(&payloads[0][..], b"\x8e\x01");
        assert_eq!(&payloads[1][..], b"\x8e\x02");

        // 重复投递应被丢弃
        events.clear();
        session
            .handle_datagram(
                &data_frame(
                    4,
                    vec![EncapsulatedPacket::reliable(
                        Bytes::from_static(b"\x8e\x01"),
                        1,
                    )],
                ),
                &mut out,
                &mut events,
            )
            .unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn split_packets_are_reassembled() {
        let mut session = connected_session();
        let mut out = Vec::new();
        let mut events = Vec::new();

        let mut first = EncapsulatedPacket::reliable(Bytes::from_static(b"\x8e"), 1);
        first.split = Some(SplitInfo {
            count: 2,
            id: 9,
            index: 0,
        });
        let mut second = EncapsulatedPacket::reliable(Bytes::from_static(b"hello"), 2);
        second.split = Some(SplitInfo {
            count: 2,
            id: 9,
            index: 1,
        });

        session
            .handle_datagram(&data_frame(2, vec![first]), &mut out, &mut events)
            .unwrap();
        assert!(events.is_empty());

        session
            .handle_datagram(&data_frame(3, vec![second]), &mut out, &mut events)
            .unwrap();
        let payload = events
            .iter()
            .find_map(|e| match e {
                SessionEvent::Game { payload, .. } => Some(payload.clone()),
                _ => None,
            })
            .expect("应重组出游戏包");
        assert_eq!(&payload[..], b"\x8ehello");
    }

    #[test]
    fn ack_clears_recovery_and_nack_resends() {
        let mut session = connected_session();
        let mut out = Vec::new();
        let mut events = Vec::new();

        let base = session.recovery_len();
        session
            .send_game(Bytes::from_static(b"\x8e\x93hi"), &mut out)
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(session.recovery_len(), base + 1);

        // ACK 掉全部已发送序号 包含 connected_session 里未确认的 SERVER_HANDSHAKE
        let last = out[0][1] as u32 | ((out[0][2] as u32) << 8) | ((out[0][3] as u32) << 16);
        let ack = Ack {
            sequences: (0..=last).collect(),
        };
        session
            .handle_datagram(&ack.encode(ID_ACK), &mut out, &mut events)
            .unwrap();
        assert_eq!(session.recovery_len(), 0);

        out.clear();
        session
            .send_game(Bytes::from_static(b"\x8e\x93yo"), &mut out)
            .unwrap();
        let seq = out[0][1] as u32 | ((out[0][2] as u32) << 8) | ((out[0][3] as u32) << 16);
        out.clear();

        let nack = Ack {
            sequences: vec![seq],
        };
        session
            .handle_datagram(&nack.encode(ID_NACK), &mut out, &mut events)
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0], 0x84);
    }
}
