use bytes::Bytes;

use crate::network::protocol::item::{ItemStack, write_slot};
use crate::network::protocol::pid::CONTAINER_SET_CONTENT;
use crate::utils::binary::BinaryWriter;

pub const SPECIAL_INVENTORY: u8 = 0;
pub const SPECIAL_ARMOR: u8 = 0x78;
pub const SPECIAL_CREATIVE: u8 = 0x79;

// 玩家背包 27 个主格 + 9 个快捷栏位置映射
#[derive(Debug, Clone)]
pub struct ContainerSetContentPacket {
    pub window_id: u8,
    pub slots: Vec<Option<ItemStack>>,
    pub hotbar: Vec<i32>,
}

impl ContainerSetContentPacket {
    pub fn inventory(slots: Vec<Option<ItemStack>>, hotbar: Vec<i32>) -> Self {
        Self {
            window_id: SPECIAL_INVENTORY,
            slots,
            hotbar,
        }
    }

    pub fn empty_inventory() -> Self {
        Self::inventory(vec![None; 27], vec![-1; 9])
    }

    pub fn empty_armor() -> Self {
        Self {
            window_id: SPECIAL_ARMOR,
            slots: vec![None; 4],
            hotbar: Vec::new(),
        }
    }

    // 0xb9 | windowid(u8) | count(u16) | 槽位(空槽为 u16 0)
    //      | 快捷栏数量(u16) | 快捷栏内容(i32 * n)
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(8 + self.slots.len() * 2 + self.hotbar.len() * 4);
        w.put_u8(CONTAINER_SET_CONTENT);
        w.put_u8(self.window_id);
        w.put_u16(self.slots.len() as u16);

        for slot in &self.slots {
            write_slot(&mut w, slot.as_ref());
        }

        // 只有玩家自己的背包带快捷栏映射
        if self.window_id == SPECIAL_INVENTORY && !self.hotbar.is_empty() {
            w.put_u16(self.hotbar.len() as u16);
            for slot in &self.hotbar {
                w.put_i32(*slot);
            }
        } else {
            w.put_u16(0);
        }

        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_inventory_layout() {
        let data = ContainerSetContentPacket::empty_inventory().encode();
        assert_eq!(data[0], CONTAINER_SET_CONTENT);
        assert_eq!(data[1], SPECIAL_INVENTORY);
        assert_eq!(&data[2..4], &27u16.to_be_bytes());
        // 27 个空槽 每个两字节
        assert!(data[4..4 + 54].iter().all(|b| *b == 0));
        assert_eq!(&data[58..60], &9u16.to_be_bytes());
        // 快捷栏全为 -1
        for chunk in data[60..].chunks_exact(4) {
            assert_eq!(i32::from_be_bytes(chunk.try_into().unwrap()), -1);
        }
        assert_eq!(data.len(), 60 + 36);
    }

    #[test]
    fn armor_window_has_no_hotbar() {
        let data = ContainerSetContentPacket::empty_armor().encode();
        assert_eq!(data[1], SPECIAL_ARMOR);
        assert_eq!(&data[2..4], &4u16.to_be_bytes());
        assert!(data[4..12].iter().all(|b| *b == 0));
        // 末尾的快捷栏数量为 0
        assert_eq!(&data[12..14], &0u16.to_be_bytes());
        assert_eq!(data.len(), 14);
    }
}
