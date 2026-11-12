use crate::utils::binary::BinaryWriter;

// 0.14 的物品槽 id 与方块 id 通用 这里只用到基本字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    pub id: i16,
    pub count: u8,
    pub damage: i16,
}

impl ItemStack {
    pub fn new(id: i16, count: u8) -> Self {
        Self {
            id,
            count,
            damage: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.id <= 0 || self.count == 0
    }
}

// 空槽只写 short 0 非空为 id(BE) count(u8) damage(BE) nbt长度(LE 0)
pub fn write_slot(w: &mut BinaryWriter, item: Option<&ItemStack>) {
    match item {
        Some(item) if !item.is_empty() => {
            w.put_u16(item.id as u16);
            w.put_u8(item.count);
            w.put_u16(item.damage as u16);
            w.put_lu16(0);
        }
        _ => w.put_u16(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_slot_is_two_bytes() {
        let mut w = BinaryWriter::new();
        write_slot(&mut w, None);
        write_slot(&mut w, Some(&ItemStack::new(0, 5)));
        assert_eq!(w.freeze(), vec![0, 0, 0, 0]);
    }

    #[test]
    fn filled_slot_layout() {
        let mut w = BinaryWriter::new();
        write_slot(&mut w, Some(&ItemStack::new(2, 3)));
        let data = w.freeze();
        assert_eq!(&data[0..2], &2u16.to_be_bytes());
        assert_eq!(data[2], 3);
        assert_eq!(&data[3..5], &[0, 0]);
        // nbt 长度是小端 0
        assert_eq!(&data[5..7], &[0, 0]);
        assert_eq!(data.len(), 7);
    }
}
