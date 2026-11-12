use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bytes::Bytes;

use crate::error::{Error, Result};
use crate::raknet::MAGIC;
use crate::utils::binary::{BinaryReader, BinaryWriter};

// 离线包
pub const ID_UNCONNECTED_PING: u8 = 0x01;
pub const ID_UNCONNECTED_PING_OPEN_CONNECTIONS: u8 = 0x02;
pub const ID_UNCONNECTED_PONG: u8 = 0x1c;

// 连接握手
pub const ID_OPEN_CONNECTION_REQUEST_1: u8 = 0x05;
pub const ID_OPEN_CONNECTION_REPLY_1: u8 = 0x06;
pub const ID_OPEN_CONNECTION_REQUEST_2: u8 = 0x07;
pub const ID_OPEN_CONNECTION_REPLY_2: u8 = 0x08;

// 封装在 DATA_PACKET 内部的握手包
pub const ID_PING: u8 = 0x00;
pub const ID_PONG: u8 = 0x03;
pub const ID_CLIENT_CONNECT: u8 = 0x09;
pub const ID_SERVER_HANDSHAKE: u8 = 0x10;
pub const ID_CLIENT_HANDSHAKE: u8 = 0x13;
pub const ID_CLIENT_DISCONNECT: u8 = 0x15;

pub const MIN_MTU: u16 = 548;
pub const MAX_MTU: u16 = 1464;

// MCPE 的地址字段为 version + 4 字节 IP 每字节取反 + u16 端口 Nukkit 的做法
fn put_address(w: &mut BinaryWriter, addr: SocketAddr) {
    w.put_u8(4);
    let octets = match addr.ip() {
        IpAddr::V4(v4) => v4.octets(),
        IpAddr::V6(_) => [0, 0, 0, 0],
    };
    for b in octets {
        w.put_u8(!b);
    }
    w.put_u16(addr.port());
}

fn get_address(r: &mut BinaryReader) -> Result<SocketAddr> {
    if r.get_u8()? != 4 {
        return Err(Error::BadAddress);
    }
    let mut octets = [0u8; 4];
    for o in &mut octets {
        *o = !r.get_u8()?;
    }
    let port = r.get_u16()?;
    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(octets)), port))
}

#[derive(Debug, Clone, Copy)]
pub struct UnconnectedPing {
    pub ping_id: u64,
}

impl UnconnectedPing {
    // 0x01/0x02 | pingID(i64 BE) | MAGIC
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_UNCONNECTED_PING && pid != ID_UNCONNECTED_PING_OPEN_CONNECTIONS {
            return Err(Error::BadPacketId(pid));
        }
        let ping_id = r.get_u64()?;
        r.check_magic(&MAGIC)?;
        Ok(Self { ping_id })
    }
}

#[derive(Debug, Clone)]
pub struct UnconnectedPong<'a> {
    pub ping_id: u64,
    pub server_id: u64,
    pub server_name: &'a str,
}

impl<'a> UnconnectedPong<'a> {
    pub fn reply_to(ping: UnconnectedPing, server_id: u64, server_name: &'a str) -> Self {
        Self {
            ping_id: ping.ping_id,
            server_id,
            server_name,
        }
    }

    // 0x1c | pingID(i64 BE) | serverID(i64 BE) | MAGIC | serverName(u16 + UTF-8)
    pub fn encode(&self) -> Result<Bytes> {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_UNCONNECTED_PONG);
        w.put_u64(self.ping_id);
        w.put_u64(self.server_id);
        w.put_bytes(&MAGIC);
        w.put_string(self.server_name)?;
        Ok(Bytes::from(w.freeze()))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenConnectionRequest1 {
    pub mtu: u16,
}

impl OpenConnectionRequest1 {
    // 0x05 | MAGIC | protocol(u8) | 填充到 MTU
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_OPEN_CONNECTION_REQUEST_1 {
            return Err(Error::BadPacketId(pid));
        }
        r.check_magic(&MAGIC)?;
        let _protocol = r.get_u8()?;
        // RakNet 用整包长度表示客户端 MTU
        let mtu = u16::try_from(buf.len()).unwrap_or(MAX_MTU);
        Ok(Self { mtu })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenConnectionReply1 {
    pub server_id: u64,
    pub mtu: u16,
}

impl OpenConnectionReply1 {
    // 0x06 | MAGIC | serverID(i64) | security(u8) | mtu(u16)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_OPEN_CONNECTION_REPLY_1);
        w.put_bytes(&MAGIC);
        w.put_u64(self.server_id);
        w.put_u8(0);
        w.put_u16(self.mtu);
        Bytes::from(w.freeze())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenConnectionRequest2 {
    pub mtu: u16,
    pub client_id: u64,
}

impl OpenConnectionRequest2 {
    // 0x07 | MAGIC | serverAddress(7B) | mtu(u16) | clientID(i64)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_OPEN_CONNECTION_REQUEST_2 {
            return Err(Error::BadPacketId(pid));
        }
        r.check_magic(&MAGIC)?;
        get_address(&mut r)?;
        let mtu = r.get_u16()?;
        let client_id = r.get_u64()?;
        Ok(Self { mtu, client_id })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenConnectionReply2 {
    pub server_id: u64,
    pub client: SocketAddr,
    pub mtu: u16,
}

impl OpenConnectionReply2 {
    // 0x08 | MAGIC | serverID(i64) | clientAddress(7B) | mtu(u16) | security(u8)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_OPEN_CONNECTION_REPLY_2);
        w.put_bytes(&MAGIC);
        w.put_u64(self.server_id);
        put_address(&mut w, self.client);
        w.put_u16(self.mtu);
        w.put_u8(0);
        Bytes::from(w.freeze())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClientConnect {
    pub client_id: u64,
    pub send_ping: u64,
    pub use_security: bool,
}

impl ClientConnect {
    // 0x09 | clientID(i64) | sendPing(i64) | useSecurity(u8)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_CLIENT_CONNECT {
            return Err(Error::BadPacketId(pid));
        }
        let client_id = r.get_u64()?;
        let send_ping = r.get_u64()?;
        let use_security = r.get_u8()? > 0;
        Ok(Self {
            client_id,
            send_ping,
            use_security,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ServerHandshake {
    pub server: SocketAddr,
    pub send_ping: u64,
    pub send_pong: u64,
}

impl ServerHandshake {
    // 0x10 | address(7B) | u16 | 10 x address(7B) | sendPing(i64) | sendPong(i64)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_SERVER_HANDSHAKE);
        put_address(&mut w, self.server);
        w.put_u16(0);
        let system = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        for _ in 0..10 {
            put_address(&mut w, system);
        }
        w.put_u64(self.send_ping);
        w.put_u64(self.send_pong);
        Bytes::from(w.freeze())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClientHandshake {
    pub send_ping: u64,
    pub send_pong: u64,
}

impl ClientHandshake {
    // 0x13 | address(7B) | 10 x address(7B) | sendPing(i64) | sendPong(i64)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_CLIENT_HANDSHAKE {
            return Err(Error::BadPacketId(pid));
        }
        get_address(&mut r)?;
        for _ in 0..10 {
            get_address(&mut r)?;
        }
        let send_ping = r.get_u64()?;
        let send_pong = r.get_u64()?;
        Ok(Self {
            send_ping,
            send_pong,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ping {
    pub ping_id: u64,
}

impl Ping {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ID_PING {
            return Err(Error::BadPacketId(pid));
        }
        Ok(Self {
            ping_id: r.get_u64()?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Pong {
    pub ping_id: u64,
}

impl Pong {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_PONG);
        w.put_u64(self.ping_id);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ping_bytes(pid: u8, ping_id: u64, magic: &[u8; 16]) -> Vec<u8> {
        let mut v = vec![pid];
        v.extend_from_slice(&ping_id.to_be_bytes());
        v.extend_from_slice(magic);
        v
    }

    #[test]
    fn pong_layout_matches_nukkit() {
        let name = "MCPE;I am Xiao_QDev;70;0.14.3;0;20";
        let ping_id = 0x0102_0304_0506_0708u64;
        let server_id = 0x1122_3344_5566_7788u64;
        let pong = UnconnectedPong {
            ping_id,
            server_id,
            server_name: name,
        };
        let buf = pong.encode().unwrap();

        assert_eq!(buf[0], ID_UNCONNECTED_PONG);
        assert_eq!(&buf[1..9], &ping_id.to_be_bytes()[..]);
        assert_eq!(&buf[9..17], &server_id.to_be_bytes()[..]);
        assert_eq!(&buf[17..33], MAGIC.as_slice());
        let name_len = u16::from_be_bytes([buf[33], buf[34]]) as usize;
        assert_eq!(name_len, name.len());
        assert_eq!(&buf[35..35 + name_len], name.as_bytes());
        assert_eq!(buf.len(), 35 + name_len);
    }

    #[test]
    fn pong_rejects_oversized_name() {
        let long = "x".repeat(65536);
        let pong = UnconnectedPong {
            ping_id: 1,
            server_id: 2,
            server_name: &long,
        };
        assert!(matches!(pong.encode(), Err(Error::StringTooLong(65536))));
    }

    #[test]
    fn ping_decodes_both_ids() {
        let ping_id = 0x0102_0304_0506_0708u64;
        for pid in [ID_UNCONNECTED_PING, ID_UNCONNECTED_PING_OPEN_CONNECTIONS] {
            let bytes = ping_bytes(pid, ping_id, &MAGIC);
            let ping = UnconnectedPing::decode(&bytes).unwrap();
            assert_eq!(ping.ping_id, ping_id);
        }
    }

    #[test]
    fn rejects_bad_magic_short_packet_and_wrong_id() {
        let bad = ping_bytes(ID_UNCONNECTED_PING, 1, &[0u8; 16]);
        assert!(matches!(
            UnconnectedPing::decode(&bad),
            Err(Error::BadMagic)
        ));
        assert!(matches!(
            UnconnectedPing::decode(&[0x01, 0x00]),
            Err(Error::TooShort { .. })
        ));
        let wrong = ping_bytes(0x05, 1, &MAGIC);
        assert!(matches!(
            UnconnectedPing::decode(&wrong),
            Err(Error::BadPacketId(0x05))
        ));
    }

    #[test]
    fn connection_request_2_roundtrip() {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_OPEN_CONNECTION_REQUEST_2);
        w.put_bytes(&MAGIC);
        put_address(&mut w, "127.0.0.1:19132".parse().unwrap());
        w.put_u16(1200);
        w.put_u64(0x1122_3344_5566_7788);
        let buf = w.freeze();

        let req = OpenConnectionRequest2::decode(&buf).unwrap();
        assert_eq!(req.mtu, 1200);
        assert_eq!(req.client_id, 0x1122_3344_5566_7788);

        let mut r = BinaryReader::new(&buf);
        r.skip(1 + 16).unwrap();
        let addr = get_address(&mut r).unwrap();
        assert_eq!(addr, "127.0.0.1:19132".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn request_1_uses_datagram_length_as_mtu() {
        let mut buf = vec![ID_OPEN_CONNECTION_REQUEST_1];
        buf.extend_from_slice(&MAGIC);
        buf.push(6);
        buf.resize(1492, 0);
        let req = OpenConnectionRequest1::decode(&buf).unwrap();
        assert_eq!(req.mtu, 1492);
    }

    #[test]
    fn client_handshake_skips_system_addresses() {
        let mut w = BinaryWriter::new();
        w.put_u8(ID_CLIENT_HANDSHAKE);
        put_address(&mut w, "192.168.1.5:50000".parse().unwrap());
        for _ in 0..10 {
            put_address(&mut w, "0.0.0.0:0".parse().unwrap());
        }
        w.put_u64(11);
        w.put_u64(22);
        let buf = w.freeze();

        let hs = ClientHandshake::decode(&buf).unwrap();
        assert_eq!(hs.send_ping, 11);
        assert_eq!(hs.send_pong, 22);
    }
}
