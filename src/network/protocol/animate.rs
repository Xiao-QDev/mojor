use bytes::Bytes;

use crate::error::{Error, Result};
use crate::network::protocol::pid::ANIMATE;
use crate::utils::binary::{BinaryReader, BinaryWriter};

// 0.14 的动作编号 挥手最常用
pub const ACTION_SWING_ARM: u8 = 1;

// 0xb2 | action(u8) | eid(i64)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimatePacket {
    pub action: u8,
    pub eid: i64,
}

impl AnimatePacket {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != ANIMATE {
            return Err(Error::BadPacketId(pid));
        }

        Ok(Self {
            action: r.get_u8()?,
            eid: r.get_i64()?,
        })
    }

    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(10);
        w.put_u8(ANIMATE);
        w.put_u8(self.action);
        w.put_i64(self.eid);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_swing_action() {
        let data = AnimatePacket {
            action: ACTION_SWING_ARM,
            eid: 5,
        }
        .encode();

        assert_eq!(data[0], ANIMATE);
        assert_eq!(data[1], ACTION_SWING_ARM);
        assert_eq!(&data[2..10], &5i64.to_be_bytes());
        assert_eq!(data.len(), 10);
    }

    #[test]
    fn decodes_roundtrip() {
        let data = AnimatePacket {
            action: ACTION_SWING_ARM,
            eid: 9,
        }
        .encode();
        let packet = AnimatePacket::decode(&data).unwrap();
        assert_eq!(packet.action, ACTION_SWING_ARM);
        assert_eq!(packet.eid, 9);
    }
}
