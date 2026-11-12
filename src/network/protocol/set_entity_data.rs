use bytes::Bytes;

use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::pid::SET_ENTITY_DATA;
use crate::utils::binary::BinaryWriter;

// 0xad | eid(i64) | metadata
pub struct SetEntityDataPacket {
    pub eid: i64,
    pub metadata: Metadata,
}

impl SetEntityDataPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(9 + self.metadata.len());
        w.put_u8(SET_ENTITY_DATA);
        w.put_i64(self.eid);
        w.put_bytes(&self.metadata.encode());
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::entity_data::DATA_FLAGS;

    #[test]
    fn layout_is_eid_then_metadata() {
        let data = SetEntityDataPacket {
            eid: 3,
            metadata: Metadata::flags(0x08),
        }
        .encode();

        assert_eq!(data[0], SET_ENTITY_DATA);
        assert_eq!(&data[1..9], &3i64.to_be_bytes());
        // 标志位 key 0 类型 byte 值 8
        assert_eq!(data[9], DATA_FLAGS);
        assert_eq!(data[10], 0x08);
        assert_eq!(data[11], 0x7f);
    }
}
