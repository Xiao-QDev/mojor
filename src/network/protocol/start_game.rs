use bytes::Bytes;

use crate::network::protocol::pid::START_GAME;
use crate::utils::binary::BinaryWriter;

// 0.14 的 StartGame 只有这些字段 比现代版精简很多
#[derive(Debug, Clone)]
pub struct StartGamePacket {
    pub seed: i32,
    pub dimension: u8,
    pub generator: i32,
    pub gamemode: i32,
    pub eid: i64,
    pub spawn: (i32, i32, i32),
    pub position: (f32, f32, f32),
    pub b1: bool,
    pub b2: bool,
    pub b3: bool,
    pub unknown: String,
}

impl StartGamePacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(START_GAME);
        w.put_i32(self.seed);
        w.put_u8(self.dimension);
        w.put_i32(self.generator);
        w.put_i32(self.gamemode);
        w.put_i64(self.eid);
        w.put_i32(self.spawn.0);
        w.put_i32(self.spawn.1);
        w.put_i32(self.spawn.2);
        w.put_f32(self.position.0);
        w.put_f32(self.position.1);
        w.put_f32(self.position.2);
        w.put_u8(self.b1 as u8);
        w.put_u8(self.b2 as u8);
        w.put_u8(self.b3 as u8);
        w.put_string(&self.unknown)
            .expect("start game 的 unknown 字段不会超过 u16 长度");
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_game_layout() {
        let packet = StartGamePacket {
            seed: -1,
            dimension: 0,
            generator: 1,
            gamemode: 1,
            eid: 0,
            spawn: (0, 4, 0),
            position: (0.5, 5.0, 0.5),
            b1: true,
            b2: true,
            b3: false,
            unknown: String::new(),
        };
        let data = packet.encode();

        assert_eq!(data[0], START_GAME);
        assert_eq!(&data[1..5], &(-1i32).to_be_bytes());
        assert_eq!(data[5], 0);
        assert_eq!(&data[6..10], &1i32.to_be_bytes());
        assert_eq!(&data[10..14], &1i32.to_be_bytes());
        assert_eq!(&data[14..22], &0i64.to_be_bytes());
        assert_eq!(&data[22..26], &0i32.to_be_bytes());
        assert_eq!(&data[26..30], &4i32.to_be_bytes());
        assert_eq!(&data[30..34], &0i32.to_be_bytes());
        assert_eq!(&data[34..38], &0.5f32.to_be_bytes());
        assert_eq!(&data[38..42], &5.0f32.to_be_bytes());
        assert_eq!(&data[42..46], &0.5f32.to_be_bytes());
        assert_eq!(&data[46..49], &[1, 1, 0]);
        assert_eq!(&data[49..], &[0, 0]);
        assert_eq!(data.len(), 51);
    }
}
