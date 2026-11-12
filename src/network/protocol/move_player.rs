use crate::error::{Error, Result};
use crate::network::protocol::pid::MOVE_PLAYER;
use crate::utils::binary::BinaryReader;

pub const MODE_NORMAL: u8 = 0;
pub const MODE_RESET: u8 = 1;
pub const MODE_ROTATION: u8 = 2;

// 0.14 客户端上报的 y 是眼睛位置 与 Nukkit 一致 减视高才是脚部坐标
pub const EYE_HEIGHT: f32 = 1.62;

// 客户端上报的自身位置 服务端内部统一用脚部坐标
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovePlayerPacket {
    pub eid: i64,
    pub position: (f32, f32, f32),
    pub yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub mode: u8,
    pub on_ground: bool,
}

impl MovePlayerPacket {
    // 0x9d | eid(i64) | x y z(f32) | yaw headYaw pitch(f32) | mode(u8) | onGround(u8)
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != MOVE_PLAYER {
            return Err(Error::BadPacketId(pid));
        }

        let eid = r.get_i64()?;
        let x = r.get_f32()?;
        let y = r.get_f32()?;
        let z = r.get_f32()?;
        let yaw = r.get_f32()?;
        let head_yaw = r.get_f32()?;
        let pitch = r.get_f32()?;
        let mode = r.get_u8()?;
        let on_ground = r.get_u8()? > 0;

        Ok(Self {
            eid,
            position: (x, y, z),
            yaw,
            head_yaw,
            pitch,
            mode,
            on_ground,
        })
    }

    // 脚部坐标 碰撞盒与距离判定都以它为准
    pub fn feet_position(&self) -> (f32, f32, f32) {
        (
            self.position.0,
            self.position.1 - EYE_HEIGHT,
            self.position.2,
        )
    }

    // 所在区块坐标
    pub fn chunk(&self) -> (i32, i32) {
        (
            (self.position.0.floor() as i32) >> 4,
            (self.position.2.floor() as i32) >> 4,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::binary::BinaryWriter;

    fn sample(x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut w = BinaryWriter::new();
        w.put_u8(MOVE_PLAYER);
        w.put_i64(0);
        w.put_f32(x);
        w.put_f32(y);
        w.put_f32(z);
        w.put_f32(90.0);
        w.put_f32(95.0);
        w.put_f32(-10.0);
        w.put_u8(MODE_NORMAL);
        w.put_u8(1);
        w.freeze().to_vec()
    }

    #[test]
    fn move_player_decodes_all_fields() {
        let buf = sample(16.5, 5.6, -0.5);
        assert_eq!(buf.len(), 35);

        let packet = MovePlayerPacket::decode(&buf).unwrap();
        assert_eq!(packet.eid, 0);
        assert_eq!(packet.position, (16.5, 5.6, -0.5));
        // 客户端 y 是眼睛位置 脚部要减视高
        assert_eq!(packet.feet_position(), (16.5, 5.6 - EYE_HEIGHT, -0.5));
        assert_eq!(packet.yaw, 90.0);
        assert_eq!(packet.head_yaw, 95.0);
        assert_eq!(packet.pitch, -10.0);
        assert_eq!(packet.mode, MODE_NORMAL);
        assert!(packet.on_ground);
    }

    // 负坐标必须向下取整 否则 (-0.5, -0.5) 会算成区块 (0, 0)
    #[test]
    fn chunk_uses_floor_for_negative_coords() {
        let chunk = MovePlayerPacket::decode(&sample(-0.5, 5.0, -17.0))
            .unwrap()
            .chunk();
        assert_eq!(chunk, (-1, -2));

        let chunk = MovePlayerPacket::decode(&sample(16.5, 5.0, 0.5))
            .unwrap()
            .chunk();
        assert_eq!(chunk, (1, 0));
    }

    #[test]
    fn move_player_rejects_wrong_pid() {
        let mut buf = sample(0.5, 5.0, 0.5);
        buf[0] = 0x90;
        assert!(matches!(
            MovePlayerPacket::decode(&buf),
            Err(Error::BadPacketId(0x90))
        ));
    }
}
