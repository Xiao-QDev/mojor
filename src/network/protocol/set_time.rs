use bytes::Bytes;

use crate::network::protocol::pid::SET_TIME;
use crate::utils::binary::BinaryWriter;

#[derive(Debug, Clone, Copy)]
pub struct SetTimePacket {
    pub time: i32,
    pub started: bool,
}

impl SetTimePacket {
    // 0x94 | time(i32) | started(u8)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(SET_TIME);
        w.put_i32(self.time);
        w.put_u8(self.started as u8);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_time_layout() {
        let data = SetTimePacket {
            time: 6000,
            started: true,
        }
        .encode();
        assert_eq!(data[0], SET_TIME);
        assert_eq!(&data[1..5], &6000i32.to_be_bytes());
        assert_eq!(data[5], 1);
    }
}
