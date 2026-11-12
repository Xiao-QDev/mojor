use bytes::Bytes;

use crate::error::{Error, Result};
use crate::network::protocol::pid::TEXT;
use crate::utils::binary::{BinaryReader, BinaryWriter};

// 与 Nukkit 0.14 的 TextPacket 类型一致
pub const TYPE_RAW: u8 = 0;
pub const TYPE_CHAT: u8 = 1;
pub const TYPE_TRANSLATION: u8 = 2;
pub const TYPE_POPUP: u8 = 3;
pub const TYPE_TIP: u8 = 4;
pub const TYPE_SYSTEM: u8 = 5;

// Nukkit 单条消息限制 255 字符
pub const MAX_MESSAGE_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPacket {
    pub kind: u8,
    pub source: String,
    pub message: String,
}

impl TextPacket {
    // 玩家聊天 客户端显示为 <source> message
    pub fn chat(source: &str, message: &str) -> Self {
        Self {
            kind: TYPE_CHAT,
            source: source.to_string(),
            message: message.to_string(),
        }
    }

    // 系统提示 没有发送者
    pub fn raw(message: &str) -> Self {
        Self {
            kind: TYPE_RAW,
            source: String::new(),
            message: message.to_string(),
        }
    }

    // 0x93 | type(u8) | 类型相关字段 只有 CHAT 与 POPUP 带来源
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != TEXT {
            return Err(Error::BadPacketId(pid));
        }

        let kind = r.get_u8()?;
        let mut source = String::new();
        let message = match kind {
            TYPE_CHAT | TYPE_POPUP => {
                source = r.get_string()?;
                r.get_string()?
            }
            TYPE_TRANSLATION => {
                let message = r.get_string()?;
                let count = r.get_u8()?;
                for _ in 0..count {
                    r.get_string()?;
                }
                message
            }
            TYPE_RAW | TYPE_TIP | TYPE_SYSTEM => r.get_string()?,
            other => return Err(Error::BadPacketId(other)),
        };

        Ok(Self {
            kind,
            source,
            message,
        })
    }

    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(8 + self.source.len() + self.message.len());
        w.put_u8(TEXT);
        w.put_u8(self.kind);
        match self.kind {
            TYPE_CHAT | TYPE_POPUP => {
                w.put_string(&self.source).expect("聊天来源不会超过 u16 长度");
                w.put_string(&self.message).expect("聊天内容不会超过 u16 长度");
            }
            TYPE_RAW | TYPE_TIP | TYPE_SYSTEM => {
                w.put_string(&self.message).expect("系统消息不会超过 u16 长度");
            }
            _ => {}
        }
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_encode_decode_roundtrip() {
        let data = TextPacket::chat("alice", "你好").encode();
        assert_eq!(data[0], TEXT);
        assert_eq!(data[1], TYPE_CHAT);
        assert_eq!(&data[2..4], &5u16.to_be_bytes());
        assert_eq!(&data[4..9], b"alice");

        let packet = TextPacket::decode(&data[..]).unwrap();
        assert_eq!(packet.kind, TYPE_CHAT);
        assert_eq!(packet.source, "alice");
        assert_eq!(packet.message, "你好");
    }

    // 原始文本只有消息字段
    #[test]
    fn raw_layout_has_no_source() {
        let data = TextPacket::raw("server").encode();
        assert_eq!(&data[..], &[TEXT, TYPE_RAW, 0, 6, b's', b'e', b'r', b'v', b'e', b'r']);
    }

    #[test]
    fn translation_message_skips_parameters() {
        let mut w = BinaryWriter::new();
        w.put_u8(TEXT);
        w.put_u8(TYPE_TRANSLATION);
        w.put_string("chat.type.text").unwrap();
        w.put_u8(1);
        w.put_string("alice").unwrap();
        let data = w.freeze();

        let packet = TextPacket::decode(&data).unwrap();
        assert_eq!(packet.kind, TYPE_TRANSLATION);
        assert_eq!(packet.message, "chat.type.text");
        assert_eq!(packet.source, "");
    }

    #[test]
    fn decode_rejects_wrong_pid() {
        let mut data = TextPacket::raw("x").encode().to_vec();
        data[0] = 0x90;
        assert!(matches!(
            TextPacket::decode(&data),
            Err(Error::BadPacketId(0x90))
        ));
    }
}
