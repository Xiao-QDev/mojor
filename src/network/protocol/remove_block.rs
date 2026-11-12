use crate::error::{Error, Result};
use crate::network::protocol::pid::REMOVE_BLOCK;
use crate::utils::binary::BinaryReader;

// 客户端挖掉方块 线序是 x z y 与其它包不同
#[derive(Debug, Clone, Copy)]
pub struct RemoveBlockPacket {
    pub eid: i64,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl RemoveBlockPacket {
    // 0x9e | eid(i64) | x(i32) | z(i32) | y(i8)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != REMOVE_BLOCK {
            return Err(Error::BadPacketId(pid));
        }

        let eid = r.get_i64()?;
        let x = r.get_i32()?;
        let z = r.get_i32()?;
        let y = r.get_u8()? as i8 as i32;

        Ok(Self { eid, x, y, z })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::binary::BinaryWriter;

    #[test]
    fn remove_block_decodes_xzy_order() {
        let mut w = BinaryWriter::new();
        w.put_u8(REMOVE_BLOCK);
        w.put_i64(1);
        w.put_i32(16);
        w.put_i32(-3);
        w.put_u8(4);
        let data = w.freeze().to_vec();

        let packet = RemoveBlockPacket::decode(&data).unwrap();
        assert_eq!(packet.eid, 1);
        assert_eq!((packet.x, packet.y, packet.z), (16, 4, -3));
    }

    #[test]
    fn negative_y_is_signed() {
        let mut w = BinaryWriter::new();
        w.put_u8(REMOVE_BLOCK);
        w.put_i64(0);
        w.put_i32(0);
        w.put_i32(0);
        w.put_u8((-4i8) as u8);
        let packet = RemoveBlockPacket::decode(&w.freeze()).unwrap();
        assert_eq!(packet.y, -4);
    }
}
