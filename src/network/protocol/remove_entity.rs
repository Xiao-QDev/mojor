use bytes::Bytes;

use crate::network::protocol::pid::REMOVE_ENTITY;
use crate::utils::binary::BinaryWriter;

// 0x99 | eid(i64)
pub struct RemoveEntityPacket {
    pub eid: i64,
}

impl RemoveEntityPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(9);
        w.put_u8(REMOVE_ENTITY);
        w.put_i64(self.eid);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_entity_layout() {
        let data = RemoveEntityPacket { eid: 9 }.encode();
        assert_eq!(data[0], REMOVE_ENTITY);
        assert_eq!(&data[1..9], &9i64.to_be_bytes());
        assert_eq!(data.len(), 9);
    }
}
