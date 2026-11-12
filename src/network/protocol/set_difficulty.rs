use bytes::Bytes;

use crate::network::protocol::pid::SET_DIFFICULTY;
use crate::utils::binary::BinaryWriter;

pub const PEACEFUL: i32 = 0;
pub const EASY: i32 = 1;
pub const NORMAL: i32 = 2;
pub const HARD: i32 = 3;

#[derive(Debug, Clone, Copy)]
pub struct SetDifficultyPacket {
    pub difficulty: i32,
}

impl SetDifficultyPacket {
    // 0xc0 | difficulty(i32)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(5);
        w.put_u8(SET_DIFFICULTY);
        w.put_i32(self.difficulty);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_difficulty_layout() {
        let data = SetDifficultyPacket { difficulty: NORMAL }.encode();
        assert_eq!(data[0], SET_DIFFICULTY);
        assert_eq!(&data[1..5], &2i32.to_be_bytes());
        assert_eq!(data.len(), 5);
    }
}
