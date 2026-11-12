use bytes::Bytes;

use crate::network::protocol::pid::FULL_CHUNK_DATA;
use crate::utils::binary::BinaryWriter;

pub const ORDER_COLUMNS: u8 = 0;
pub const ORDER_LAYERED: u8 = 1;

#[derive(Debug, Clone)]
pub struct FullChunkDataPacket {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub order: u8,
    pub data: Bytes,
}

impl FullChunkDataPacket {
    // 0xbf | chunkX(i32) | chunkZ(i32) | order(u8) | length(i32) | data
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(1 + 4 + 4 + 1 + 4 + self.data.len());
        w.put_u8(FULL_CHUNK_DATA);
        w.put_i32(self.chunk_x);
        w.put_i32(self.chunk_z);
        w.put_u8(self.order);
        w.put_i32(self.data.len() as i32);
        w.put_bytes(&self.data);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_chunk_layout() {
        let packet = FullChunkDataPacket {
            chunk_x: -2,
            chunk_z: 3,
            order: ORDER_COLUMNS,
            data: Bytes::from_static(b"abc"),
        };
        let data = packet.encode();
        assert_eq!(data[0], FULL_CHUNK_DATA);
        assert_eq!(&data[1..5], &(-2i32).to_be_bytes());
        assert_eq!(&data[5..9], &3i32.to_be_bytes());
        assert_eq!(data[9], ORDER_COLUMNS);
        assert_eq!(&data[10..14], &3i32.to_be_bytes());
        assert_eq!(&data[14..], b"abc");
    }
}
