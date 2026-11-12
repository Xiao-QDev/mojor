use bytes::Bytes;
use tracing::warn;

use crate::error::{Error, Result};
use crate::network::protocol::pid::LOGIN;
use crate::utils::binary::BinaryReader;

// 0.14 的皮肤只有 model 与像素数据 没有披风与几何数据
#[derive(Debug, Clone)]
pub struct Skin {
    pub model: String,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct LoginPacket {
    pub username: String,
    pub protocol1: i32,
    pub protocol2: i32,
    pub client_id: i64,
    pub client_uuid: [u8; 16],
    pub server_address: String,
    pub client_secret: Bytes,
    pub skin: Skin,
}

// 失败时记录字段名 起始偏移 以及该处原始字节 便于对照客户端实际布局
fn field<T>(
    buf: &[u8],
    r: &mut BinaryReader<'_>,
    name: &'static str,
    f: impl FnOnce(&mut BinaryReader<'_>) -> Result<T>,
) -> Result<T> {
    let offset = r.offset();
    let result = f(r);
    if let Err(e) = &result {
        warn!(
            field = name,
            offset,
            peek = %hex_at(buf, offset, 64),
            error = %e,
            "登录包字段解析失败"
        );
    }
    result
}

// [u16 长度][原始字节] 的字段 内容可能是二进制 不能按字符串读
fn bytes_field(
    buf: &[u8],
    r: &mut BinaryReader<'_>,
    name: &'static str,
    len: usize,
) -> Result<Bytes> {
    let offset = r.offset();
    match r.get_bytes(len) {
        Ok(b) => Ok(Bytes::copy_from_slice(b)),
        Err(e) => {
            warn!(
                field = name,
                offset,
                peek = %hex_at(buf, offset, 64),
                error = %e,
                "登录包字段解析失败"
            );
            Err(e)
        }
    }
}

fn hex_at(buf: &[u8], offset: usize, limit: usize) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(limit * 3);
    if offset < buf.len() {
        for b in buf[offset..].iter().take(limit) {
            let _ = write!(out, "{b:02x} ");
        }
    }
    out.trim_end().to_string()
}

impl LoginPacket {
    // buf 以包 ID 开头
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != LOGIN {
            return Err(Error::BadPacketId(pid));
        }

        let username = field(buf, &mut r, "username", |r| r.get_string())?;
        let protocol1 = field(buf, &mut r, "protocol1", |r| r.get_i32())?;
        let protocol2 = field(buf, &mut r, "protocol2", |r| r.get_i32())?;
        let client_id = field(buf, &mut r, "client_id", |r| r.get_i64())?;
        let client_uuid = field(buf, &mut r, "client_uuid", |r| r.get_uuid())?;
        let server_address = field(buf, &mut r, "server_address", |r| r.get_string())?;

        // clientSecret 是 u16 长度前缀的 16 字节随机数 不是文本
        let secret_len = field(buf, &mut r, "client_secret_len", |r| r.get_u16())? as usize;
        let client_secret = bytes_field(buf, &mut r, "client_secret", secret_len)?;

        let model = field(buf, &mut r, "skin_model", |r| r.get_string())?;
        let skin_len = field(buf, &mut r, "skin_len", |r| r.get_u16())? as usize;
        let data = bytes_field(buf, &mut r, "skin_data", skin_len)?;

        Ok(Self {
            username,
            protocol1,
            protocol2,
            client_id,
            client_uuid,
            server_address,
            client_secret,
            skin: Skin { model, data },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::binary::BinaryWriter;

    // 取自 0.14 客户端抓包 该字段不含可打印字符
    const SECRET: [u8; 16] = [
        0x37, 0x10, 0xce, 0x62, 0xdf, 0x42, 0x95, 0x79, 0xf9, 0xcc, 0x4f, 0xf5, 0xea, 0x0e, 0x32,
        0xd7,
    ];

    fn sample(skin_len: usize) -> Vec<u8> {
        let mut w = BinaryWriter::new();
        w.put_u8(LOGIN);
        w.put_string("Xiao_QDev").unwrap();
        w.put_i32(70);
        w.put_i32(70);
        w.put_i64(0x1122_3344_5566_7788);
        w.put_bytes(&[0xaa; 16]);
        w.put_string("127.0.0.1:19132").unwrap();
        // 真实客户端这里是 16 字节二进制随机数 不是文本
        w.put_u16(16);
        w.put_bytes(&SECRET);
        w.put_string("Standard_Steve").unwrap();
        w.put_u16(skin_len as u16);
        w.put_bytes(&vec![0x7f; skin_len]);
        w.freeze().to_vec()
    }

    #[test]
    fn login_packet_decodes_all_fields() {
        let buf = sample(8192);
        let login = LoginPacket::decode(&buf).unwrap();

        assert_eq!(login.username, "Xiao_QDev");
        assert_eq!(login.protocol1, 70);
        assert_eq!(login.protocol2, 70);
        assert_eq!(login.client_id, 0x1122_3344_5566_7788);
        assert_eq!(login.client_uuid, [0xaa; 16]);
        assert_eq!(login.server_address, "127.0.0.1:19132");
        assert_eq!(&login.client_secret[..], &SECRET);
        assert_eq!(login.skin.model, "Standard_Steve");
        assert_eq!(login.skin.data.len(), 8192);
        assert!(login.skin.data.iter().all(|b| *b == 0x7f));
    }

    // 复刻真机抓包 协议 70 用户名 mineadc 64x64 原始 RGBA 皮肤
    #[test]
    fn real_client_login_packet_decodes() {
        let mut w = BinaryWriter::new();
        w.put_u8(LOGIN);
        w.put_string("mineadc").unwrap();
        w.put_i32(70);
        w.put_i32(70);
        w.put_bytes(&[0xae, 0x2d, 0x00, 0x08, 0x1d, 0x76, 0xbb, 0x90]);
        w.put_bytes(&[
            0xef, 0xc2, 0xdc, 0x29, 0x86, 0x4c, 0x35, 0xb8, 0xaf, 0x25, 0xcd, 0x15, 0x64, 0xff,
            0xce, 0x3c,
        ]);
        w.put_string("192.168.31.126:19132").unwrap();
        w.put_u16(16);
        w.put_bytes(&SECRET);
        w.put_string("Standard_CustomSlim").unwrap();
        w.put_u16(16384);
        w.put_bytes(&vec![0x40; 16384]);
        let buf = w.freeze().to_vec();
        // 与抓包长度一致 含 0x8e 前缀
        assert_eq!(buf.len() + 1, 16490);

        let login = LoginPacket::decode(&buf).unwrap();
        assert_eq!(login.username, "mineadc");
        assert_eq!(login.protocol1, 70);
        assert_eq!(login.protocol2, 70);
        assert_eq!(login.server_address, "192.168.31.126:19132");
        assert_eq!(&login.client_secret[..], &SECRET);
        assert_eq!(login.skin.model, "Standard_CustomSlim");
        assert_eq!(login.skin.data.len(), 16384);
    }

    #[test]
    fn login_packet_rejects_truncated_skin() {
        let mut buf = sample(8192);
        buf.truncate(buf.len() - 100);
        assert!(LoginPacket::decode(&buf).is_err());
    }
}
