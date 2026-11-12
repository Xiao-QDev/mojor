use bytes::Bytes;

use crate::error::{Error, Result};
use crate::network::protocol::pid::{CHUNK_RADIUS_UPDATE, REQUEST_CHUNK_RADIUS};
use crate::utils::binary::{BinaryReader, BinaryWriter};

#[derive(Debug, Clone, Copy)]
pub struct RequestChunkRadiusPacket {
    pub radius: i32,
}

impl RequestChunkRadiusPacket {
    // 0xc8 | radius(i32)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != REQUEST_CHUNK_RADIUS {
            return Err(Error::BadPacketId(pid));
        }
        Ok(Self {
            radius: r.get_i32()?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChunkRadiusUpdatePacket {
    pub radius: i32,
}

impl ChunkRadiusUpdatePacket {
    // 0xc9 | radius(i32)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(CHUNK_RADIUS_UPDATE);
        w.put_i32(self.radius);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_radius_roundtrip() {
        let request = [REQUEST_CHUNK_RADIUS, 0, 0, 0, 8];
        assert_eq!(RequestChunkRadiusPacket::decode(&request).unwrap().radius, 8);

        let update = ChunkRadiusUpdatePacket { radius: 4 }.encode();
        assert_eq!(&update[..], &[CHUNK_RADIUS_UPDATE, 0, 0, 0, 4]);
    }
}
