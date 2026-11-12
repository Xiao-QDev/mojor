use bytes::Bytes;

use crate::network::protocol::pid::UPDATE_BLOCK;
use crate::utils::binary::BinaryWriter;

// 通知客户端刷新方块 邻居与网络标志都置位
pub const FLAG_ALL: u8 = 0b0011;

#[derive(Debug, Clone, Copy)]
pub struct UpdateBlockEntry {
    pub x: i32,
    pub y: u8,
    pub z: i32,
    pub block_id: u8,
    pub block_data: u8,
    pub flags: u8,
}

#[derive(Debug, Clone)]
pub struct UpdateBlockPacket {
    pub records: Vec<UpdateBlockEntry>,
}

impl UpdateBlockPacket {
    pub fn single(x: i32, y: i32, z: i32, block_id: u8, block_data: u8) -> Self {
        Self {
            records: vec![UpdateBlockEntry {
                x,
                y: y as u8,
                z,
                block_id,
                block_data,
                flags: FLAG_ALL,
            }],
        }
    }

    // 0x9f | count(i32) | [x(i32) | z(i32) | y(u8) | id(u8) | (flags << 4) | data]
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(5 + self.records.len() * 11);
        w.put_u8(UPDATE_BLOCK);
        w.put_i32(self.records.len() as i32);

        for record in &self.records {
            w.put_i32(record.x);
            w.put_i32(record.z);
            w.put_u8(record.y);
            w.put_u8(record.block_id);
            w.put_u8((record.flags << 4) | (record.block_data & 0x0f));
        }

        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_block_layout() {
        let data = UpdateBlockPacket::single(1, 5, -2, 3, 2).encode();
        assert_eq!(data[0], UPDATE_BLOCK);
        assert_eq!(&data[1..5], &1i32.to_be_bytes());
        assert_eq!(&data[5..9], &1i32.to_be_bytes());
        assert_eq!(&data[9..13], &(-2i32).to_be_bytes());
        assert_eq!(data[13], 5);
        assert_eq!(data[14], 3);
        assert_eq!(data[15], (FLAG_ALL << 4) | 2);
        assert_eq!(data.len(), 16);
    }
}
