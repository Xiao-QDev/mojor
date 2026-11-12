use bytes::Bytes;

use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::pid::ADD_PLAYER;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone)]
pub struct AddPlayerPacket {
    pub uuid: [u8; 16],
    pub username: String,
    pub eid: i64,
    pub position: (f32, f32, f32),
    pub yaw: f32,
    pub pitch: f32,
    pub metadata: Metadata,
}

impl AddPlayerPacket {
    // 0x96 | uuid(16) | username | eid(i64) | x y z(f32) | 速度(f32 x3)
    //      | yaw | headYaw | pitch(f32) | 手持物品(空槽 u16) | 元数据
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(32 + self.username.len() + self.metadata.len());
        w.put_u8(ADD_PLAYER);
        w.put_bytes(&self.uuid);
        w.put_string(&self.username)
            .expect("玩家名不会超过 u16 长度");
        w.put_i64(self.eid);
        w.put_f32(self.position.0);
        w.put_f32(self.position.1);
        w.put_f32(self.position.2);
        w.put_f32(0.0);
        w.put_f32(0.0);
        w.put_f32(0.0);
        w.put_f32(self.yaw);
        w.put_f32(self.yaw);
        w.put_f32(self.pitch);
        // 空槽直接写 short 0
        w.put_u16(0);
        w.put_bytes(&self.metadata.encode());
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_player_layout() {
        let packet = AddPlayerPacket {
            uuid: [0xaa; 16],
            username: "bob".into(),
            eid: 2,
            position: (0.5, 5.0, 0.5),
            yaw: 90.0,
            pitch: -10.0,
            metadata: Metadata::player("bob"),
        };
        let data = packet.encode();

        assert_eq!(data[0], ADD_PLAYER);
        assert_eq!(&data[1..17], &[0xaa; 16]);
        assert_eq!(&data[17..19], &3u16.to_be_bytes());
        assert_eq!(&data[19..22], b"bob");
        assert_eq!(&data[22..30], &2i64.to_be_bytes());
        assert_eq!(&data[30..34], &0.5f32.to_be_bytes());
        assert_eq!(&data[34..38], &5.0f32.to_be_bytes());
        assert_eq!(&data[38..42], &0.5f32.to_be_bytes());
        // 三个速度分量
        assert_eq!(&data[42..54], &[0u8; 12]);
        assert_eq!(&data[54..58], &90.0f32.to_be_bytes());
        assert_eq!(&data[58..62], &90.0f32.to_be_bytes());
        assert_eq!(&data[62..66], &(-10.0f32).to_be_bytes());
        // 空槽
        assert_eq!(&data[66..68], &[0, 0]);
        // 元数据以 0x7f 结尾
        assert_eq!(*data.last().unwrap(), 0x7f);
        assert_eq!(data.len(), 68 + Metadata::player("bob").len());
    }
}
