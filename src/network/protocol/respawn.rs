use bytes::Bytes;

use crate::network::protocol::pid::RESPAWN;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct RespawnPacket {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl RespawnPacket {
    // 0xb3 | x(f32) | y(f32) | z(f32)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(RESPAWN);
        w.put_f32(self.x);
        w.put_f32(self.y);
        w.put_f32(self.z);
        Bytes::from(w.freeze())
    }
}
