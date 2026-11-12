use crate::error::{Error, Result};
use crate::network::protocol::pid::USE_ITEM;
use crate::utils::binary::BinaryReader;

// 客户端点击方块 服务器据此在相邻格放方块
#[derive(Debug, Clone, Copy)]
pub struct UseItemPacket {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: u8,
    pub slot: i32,
    pub item_id: i16,
    pub item_count: u8,
    pub item_damage: i16,
}

impl UseItemPacket {
    // 0xaa | x y z(i32) | face(u8) | 三个命中点(f32) | 三个玩家坐标(f32)
    //      | slot(i32) | 手持物品槽
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        let pid = r.get_u8()?;
        if pid != USE_ITEM {
            return Err(Error::BadPacketId(pid));
        }

        let x = r.get_i32()?;
        let y = r.get_i32()?;
        let z = r.get_i32()?;
        let face = r.get_u8()?;
        for _ in 0..6 {
            r.get_f32()?;
        }
        let slot = r.get_i32()?;

        let item_id = r.get_u16()? as i16;
        let (item_count, item_damage) = if item_id <= 0 {
            (0, 0)
        } else {
            let count = r.get_u8()?;
            let damage = r.get_u16()? as i16;
            // NBT 长度是小端 空 NBT 时长度为 0
            let nbt_len = r.get_lu16()? as usize;
            r.skip(nbt_len)?;
            (count, damage)
        };

        Ok(Self {
            x,
            y,
            z,
            face,
            slot,
            item_id,
            item_count,
            item_damage,
        })
    }

    // 点击面 0 下 1 上 2 北 3 南 4 西 5 东 对应的目标格
    pub fn target_block(&self) -> Option<(i32, i32, i32)> {
        let (x, y, z) = (self.x, self.y, self.z);
        Some(match self.face {
            0 => (x, y - 1, z),
            1 => (x, y + 1, z),
            2 => (x, y, z - 1),
            3 => (x, y, z + 1),
            4 => (x - 1, y, z),
            5 => (x + 1, y, z),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::binary::BinaryWriter;

    fn sample(face: u8, item_id: i16) -> Vec<u8> {
        let mut w = BinaryWriter::new();
        w.put_u8(USE_ITEM);
        w.put_i32(4);
        w.put_i32(4);
        w.put_i32(0);
        w.put_u8(face);
        for _ in 0..6 {
            w.put_f32(0.0);
        }
        w.put_i32(0);
        if item_id <= 0 {
            w.put_u16(0);
        } else {
            w.put_u16(item_id as u16);
            w.put_u8(1);
            w.put_u16(0);
            w.put_u16(0);
        }
        w.freeze().to_vec()
    }

    #[test]
    fn use_item_decodes_held_item() {
        let packet = UseItemPacket::decode(&sample(1, 3)).unwrap();
        assert_eq!((packet.x, packet.y, packet.z), (4, 4, 0));
        assert_eq!(packet.face, 1);
        assert_eq!(packet.item_id, 3);
        assert_eq!(packet.item_count, 1);
    }

    #[test]
    fn empty_hand_has_no_item() {
        let packet = UseItemPacket::decode(&sample(1, 0)).unwrap();
        assert_eq!(packet.item_id, 0);
        assert_eq!(packet.item_count, 0);
    }

    #[test]
    fn face_maps_to_neighbor() {
        let base = UseItemPacket {
            x: 1,
            y: 5,
            z: -1,
            face: 0,
            slot: 0,
            item_id: 1,
            item_count: 1,
            item_damage: 0,
        };
        assert_eq!(base.target_block(), Some((1, 4, -1)));
        assert_eq!(
            UseItemPacket { face: 5, ..base }.target_block(),
            Some((2, 5, -1))
        );
        assert_eq!(UseItemPacket { face: 9, ..base }.target_block(), None);
    }
}
