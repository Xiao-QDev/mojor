use bytes::Bytes;

use crate::network::protocol::pid::MOVE_ENTITY;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct MoveEntityEntry {
    pub eid: i64,
    pub position: (f32, f32, f32),
    pub yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone)]
pub struct MoveEntityPacket {
    pub entries: Vec<MoveEntityEntry>,
}

impl MoveEntityPacket {
    // 0x9c | count(i32) | [eid(i64) | x y z yaw headYaw pitch(f32)]
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(4 + self.entries.len() * 32);
        w.put_u8(MOVE_ENTITY);
        w.put_i32(self.entries.len() as i32);

        for entry in &self.entries {
            w.put_i64(entry.eid);
            w.put_f32(entry.position.0);
            w.put_f32(entry.position.1);
            w.put_f32(entry.position.2);
            w.put_f32(entry.yaw);
            w.put_f32(entry.head_yaw);
            w.put_f32(entry.pitch);
        }

        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_entity_layout() {
        let packet = MoveEntityPacket {
            entries: vec![MoveEntityEntry {
                eid: 5,
                position: (1.5, 6.0, -2.5),
                yaw: 45.0,
                head_yaw: 50.0,
                pitch: 0.0,
            }],
        };
        let data = packet.encode();

        assert_eq!(data[0], MOVE_ENTITY);
        assert_eq!(&data[1..5], &1i32.to_be_bytes());
        assert_eq!(&data[5..13], &5i64.to_be_bytes());
        assert_eq!(&data[13..17], &1.5f32.to_be_bytes());
        assert_eq!(&data[17..21], &6.0f32.to_be_bytes());
        assert_eq!(&data[21..25], &(-2.5f32).to_be_bytes());
        assert_eq!(&data[25..29], &45.0f32.to_be_bytes());
        assert_eq!(&data[29..33], &50.0f32.to_be_bytes());
        assert_eq!(&data[33..37], &0.0f32.to_be_bytes());
        assert_eq!(data.len(), 37);
    }
}
