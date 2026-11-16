use crate::error::Result;
use crate::utils::binary::{BinaryReader, BinaryWriter};

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

// 读一个物品槽 空槽只有 short 0 客户端上报的槽位与 ContainerSetSlot 用这个
pub fn read_slot(r: &mut BinaryReader<'_>) -> Result<ItemStack> {
    let id = r.get_u16()? as i16;
    if id <= 0 {
        return Ok(ItemStack { id: 0, count: 0, damage: 0 });
    }
    let count = r.get_u8()?.max(1);
    let damage = r.get_u16()? as i16;
    let nbt_len = r.get_lu16()? as usize;
    r.skip(nbt_len)?;
    // 客户端用 0xFFFF(-1) 表示通配 damage 直接写回背包会让客户端渲染崩溃
    // 落到服务端背包的一律归一化成具体值
    let damage = if damage < 0 { 0 } else { damage };
    Ok(ItemStack { id, count, damage })
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
