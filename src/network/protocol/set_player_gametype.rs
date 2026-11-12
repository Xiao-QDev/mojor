use bytes::Bytes;

use crate::network::protocol::pid::SET_PLAYER_GAMETYPE;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct SetPlayerGameTypePacket {
    pub gamemode: i32,
}

impl SetPlayerGameTypePacket {
    // 0xc2 | gamemode(i32) 0 生存 1 创造
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(5);
        w.put_u8(SET_PLAYER_GAMETYPE);
        w.put_i32(self.gamemode);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_player_gametype_layout() {
        let data = SetPlayerGameTypePacket { gamemode: 0 }.encode();
        assert_eq!(data[0], SET_PLAYER_GAMETYPE);
        assert_eq!(&data[1..5], &0i32.to_be_bytes());
        assert_eq!(data.len(), 5);
    }
}
