use crate::error::{Error, Result};
use crate::network::protocol::pid::PLAYER_ACTION;
use crate::utils::binary::BinaryReader;

// 与 Nukkit PlayerActionPacket 的动作编号一致
pub const ACTION_START_SPRINT: i32 = 9;
pub const ACTION_STOP_SPRINT: i32 = 10;
pub const ACTION_START_SNEAK: i32 = 11;
pub const ACTION_STOP_SNEAK: i32 = 12;

// 0xab | eid(i64) | action(i32) | x(i32) | y(i32) | z(i32) | face(i32)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerActionPacket {
    pub eid: i64,
    pub action: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i32,
}

impl PlayerActionPacket {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != PLAYER_ACTION {
            return Err(Error::BadPacketId(pid));
        }

        Ok(Self {
            eid: r.get_i64()?,
            action: r.get_i32()?,
            x: r.get_i32()?,
            y: r.get_i32()?,
            z: r.get_i32()?,
            face: r.get_i32()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(action: i32) -> Vec<u8> {
        let mut data = vec![PLAYER_ACTION];
        data.extend_from_slice(&7i64.to_be_bytes());
        data.extend_from_slice(&action.to_be_bytes());
        for _ in 0..4 {
            data.extend_from_slice(&0i32.to_be_bytes());
        }
        data
    }

    #[test]
    fn decodes_sneak_action() {
        let packet = PlayerActionPacket::decode(&sample(ACTION_START_SNEAK)).unwrap();
        assert_eq!(packet.eid, 7);
        assert_eq!(packet.action, ACTION_START_SNEAK);
        assert_eq!((packet.x, packet.y, packet.z, packet.face), (0, 0, 0, 0));
        assert_eq!(sample(ACTION_STOP_SNEAK).len(), 29);
    }

    #[test]
    fn rejects_wrong_pid() {
        let mut data = sample(ACTION_START_SPRINT);
        data[0] = 0xaa;
        assert!(matches!(
            PlayerActionPacket::decode(&data),
            Err(Error::BadPacketId(0xaa))
        ));
    }
}
