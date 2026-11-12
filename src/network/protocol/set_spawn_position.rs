use bytes::Bytes;

use crate::network::protocol::pid::SET_SPAWN_POSITION;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct SetSpawnPositionPacket {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl SetSpawnPositionPacket {
    // 0xb1 | x(i32) | y(i32) | z(i32)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(SET_SPAWN_POSITION);
        w.put_i32(self.x);
        w.put_i32(self.y);
        w.put_i32(self.z);
        Bytes::from(w.freeze())
    }
}
