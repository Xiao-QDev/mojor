use bytes::Bytes;

use crate::network::protocol::item::{ItemStack, write_slot};
use crate::network::protocol::pid::{ADD_ITEM_ENTITY, TAKE_ITEM_ENTITY};
use crate::utils::binary::BinaryWriter;

// 0x9a | eid(i64) | 物品槽 | x y z(f32) | 速度(f32 x3)
pub struct AddItemEntityPacket {
    pub eid: i64,
    pub item: ItemStack,
    pub position: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
}

impl AddItemEntityPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(40);
        w.put_u8(ADD_ITEM_ENTITY);
        w.put_i64(self.eid);
        write_slot(&mut w, Some(&self.item));
        w.put_f32(self.position.0);
        w.put_f32(self.position.1);
        w.put_f32(self.position.2);
        w.put_f32(self.velocity.0);
        w.put_f32(self.velocity.1);
        w.put_f32(self.velocity.2);
        Bytes::from(w.freeze())
    }
}

// 0x9b | 物品实体 eid(i64) | 拾取者 eid(i64) 0 表示接收者自己
pub struct TakeItemEntityPacket {
    pub item_eid: i64,
    pub collector_eid: i64,
}

impl TakeItemEntityPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(17);
        w.put_u8(TAKE_ITEM_ENTITY);
        w.put_i64(self.item_eid);
        w.put_i64(self.collector_eid);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_item_entity_layout() {
        let data = AddItemEntityPacket {
            eid: 9,
            item: ItemStack::new(2, 1),
            position: (0.5, 4.5, 0.5),
            velocity: (0.0, 0.2, 0.0),
        }
        .encode();

        assert_eq!(data[0], ADD_ITEM_ENTITY);
        assert_eq!(&data[1..9], &9i64.to_be_bytes());
        assert_eq!(&data[9..11], &2u16.to_be_bytes());
        assert_eq!(data[11], 1);
        assert_eq!(&data[12..14], &[0, 0]);
        assert_eq!(&data[14..16], &[0, 0]);
        assert_eq!(&data[16..20], &0.5f32.to_be_bytes());
        assert_eq!(data.len(), 40);
    }

    // 线上顺序是 物品实体 在前 拾取者 在后
    #[test]
    fn take_item_entity_layout() {
        let data = TakeItemEntityPacket {
            item_eid: 9,
            collector_eid: 1,
        }
        .encode();
        assert_eq!(data[0], TAKE_ITEM_ENTITY);
        assert_eq!(&data[1..9], &9i64.to_be_bytes());
        assert_eq!(&data[9..17], &1i64.to_be_bytes());
    }
}
