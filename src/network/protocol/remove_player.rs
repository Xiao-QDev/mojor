use bytes::Bytes;

use crate::network::protocol::pid::REMOVE_PLAYER;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct RemovePlayerPacket {
    pub eid: i64,
    pub uuid: [u8; 16],
}

impl RemovePlayerPacket {
    // 0x97 | eid(i64) | uuid(16)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(25);
        w.put_u8(REMOVE_PLAYER);
        w.put_i64(self.eid);
        w.put_bytes(&self.uuid);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_player_layout() {
        let data = RemovePlayerPacket {
            eid: 7,
            uuid: [0xcc; 16],
        }
        .encode();
        assert_eq!(data[0], REMOVE_PLAYER);
        assert_eq!(&data[1..9], &7i64.to_be_bytes());
        assert_eq!(&data[9..25], &[0xcc; 16]);
        assert_eq!(data.len(), 25);
    }
}
