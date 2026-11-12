use bytes::Bytes;

use crate::network::protocol::pid::PLAY_STATUS;
use crate::utils::binary::BinaryWriter;

pub const LOGIN_SUCCESS: i32 = 0;
pub const LOGIN_FAILED_CLIENT: i32 = 1;
pub const LOGIN_FAILED_SERVER: i32 = 2;
pub const PLAYER_SPAWN: i32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct PlayStatusPacket {
    pub status: i32,
}

impl PlayStatusPacket {
    // 0x90 | status(i32 BE)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(PLAY_STATUS);
        w.put_i32(self.status);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_success_layout() {
        let data = PlayStatusPacket {
            status: LOGIN_SUCCESS,
        }
        .encode();
        assert_eq!(&data[..], &[0x90, 0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn player_spawn_layout() {
        let data = PlayStatusPacket {
            status: PLAYER_SPAWN,
        }
        .encode();
        assert_eq!(&data[..], &[0x90, 0x00, 0x00, 0x00, 0x03]);
    }
}
