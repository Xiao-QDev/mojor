// 容器与合成协议 照搬 Nukkit 0.14 的 ContainerOpen/Close/SetSlot
// 以及 CraftingData(下发配方表) / CraftingEvent(客户端上报合成)
use bytes::Bytes;

use crate::error::{Error, Result};
use crate::item::inventory::CraftingManager;
use crate::network::protocol::item::{ItemStack, read_slot, write_slot};
use crate::network::protocol::pid::{
    CONTAINER_CLOSE, CONTAINER_OPEN, CONTAINER_SET_DATA, CONTAINER_SET_SLOT, CRAFTING_DATA,
    CRAFTING_EVENT,
};
use crate::utils::binary::{BinaryReader, BinaryWriter};

pub const ENTRY_SHAPELESS: i32 = 0;
pub const ENTRY_SHAPED: i32 = 1;
pub const ENTRY_FURNACE: i32 = 2;
pub const ENTRY_FURNACE_DATA: i32 = 3;

// 玩家自己的背包窗口
pub const WINDOW_INVENTORY: u8 = 0;

// 0xb5 | windowid | type | slots(short) | x y z(int) | entityId(long)
#[derive(Debug, Clone, Copy)]
pub struct ContainerOpenPacket {
    pub window_id: u8,
    pub kind: u8,
    pub slots: u16,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub entity_id: i64,
}

impl ContainerOpenPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(CONTAINER_OPEN);
        w.put_u8(self.window_id);
        w.put_u8(self.kind);
        w.put_u16(self.slots);
        w.put_i32(self.x);
        w.put_i32(self.y);
        w.put_i32(self.z);
        w.put_i64(self.entity_id);
        Bytes::from(w.freeze())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContainerClosePacket {
    pub window_id: u8,
}

impl ContainerClosePacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(CONTAINER_CLOSE);
        w.put_u8(self.window_id);
        Bytes::from(w.freeze())
    }
}

// 0xb7 | windowid | slot(short) | hotbarSlot(short) | slot
#[derive(Debug, Clone, Copy)]
pub struct ContainerSetSlotPacket {
    pub window_id: u8,
    pub slot: i16,
    pub hotbar_slot: i16,
    pub item: Option<ItemStack>,
}

impl ContainerSetSlotPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::new();
        w.put_u8(CONTAINER_SET_SLOT);
        w.put_u8(self.window_id);
        w.put_u16(self.slot as u16);
        w.put_u16(self.hotbar_slot as u16);
        write_slot(&mut w, self.item.as_ref());
        Bytes::from(w.freeze())
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        if r.get_u8()? != CONTAINER_SET_SLOT {
            return Err(Error::BadPacketId(CONTAINER_SET_SLOT));
        }
        let window_id = r.get_u8()?;
        let slot = r.get_u16()? as i16;
        let hotbar_slot = r.get_u16()? as i16;
        let item = read_slot(&mut r)?;
        Ok(Self {
            window_id,
            slot,
            hotbar_slot,
            item: Some(item),
        })
    }
}

// 熔炉进度条 0xb8 | windowid | property(short) | value(short)
// property 0=烧制进度 1=燃料余量 与 Nukkit BlockEntityFurnace 一致
#[derive(Debug, Clone, Copy)]
pub struct ContainerSetDataPacket {
    pub window_id: u8,
    pub property: u16,
    pub value: u16,
}

impl ContainerSetDataPacket {
    pub fn encode(&self) -> Bytes {
        let mut w = BinaryWriter::with_capacity(5);
        w.put_u8(CONTAINER_SET_DATA);
        w.put_u8(self.window_id);
        w.put_u16(self.property);
        w.put_u16(self.value);
        Bytes::from(w.freeze())
    }
}

// 客户端合成完成时上报 服务端据此校验扣料并回背包
#[derive(Debug, Clone)]
pub struct CraftingEventPacket {
    pub window_id: u8,
    pub kind: i32,
    pub uuid: [u8; 16],
    pub input: Vec<ItemStack>,
    pub output: Vec<ItemStack>,
}

impl CraftingEventPacket {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        let mut r = BinaryReader::new(buf);
        if r.get_u8()? != CRAFTING_EVENT {
            return Err(Error::BadPacketId(CRAFTING_EVENT));
        }
        let window_id = r.get_u8()?;
        let kind = r.get_i32()?;
        let uuid = r.get_uuid()?;
        let input = read_slots(&mut r)?;
        let output = read_slots(&mut r)?;
        Ok(Self {
            window_id,
            kind,
            uuid,
            input,
            output,
        })
    }
}

fn read_slots(r: &mut BinaryReader<'_>) -> Result<Vec<ItemStack>> {
    let count = r.get_i32()?;
    if !(0..=128).contains(&count) {
        return Err(Error::BadPacketId(CRAFTING_EVENT));
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(read_slot(r)?);
    }
    Ok(out)
}

// 配方 id 客户端只当不透明值回传 用稳定序号生成即可
// CraftingManager::recipe_by_uuid 按同一套编号反查 别改顺序
pub fn recipe_uuid(index: usize) -> [u8; 16] {
    let mut uuid = [0u8; 16];
    uuid[..8].copy_from_slice(b"MojorRcp");
    uuid[8..].copy_from_slice(&(index as u64).to_be_bytes());
    uuid
}

// 0xba | entries(int) | 每条: type(int) | body长度(int) | body | cleanRecipes(byte)
// 没下发这个包 客户端合成网格不会产出任何东西
#[derive(Debug, Clone)]
pub struct CraftingDataPacket {
    pub clean_recipes: bool,
}

impl CraftingDataPacket {
    pub fn encode(&self, manager: &CraftingManager) -> Bytes {
        let mut entries: Vec<(i32, Vec<u8>)> = Vec::new();
        let mut index = 0usize;

        for recipe in &manager.shapeless {
            let mut body = BinaryWriter::new();
            let ingredients = recipe.ingredient_stacks();
            body.put_i32(ingredients.len() as i32);
            for stack in &ingredients {
                write_slot(&mut body, Some(stack));
            }
            body.put_i32(1);
            write_slot(&mut body, Some(&recipe.result));
            body.put_bytes(&recipe_uuid(index));
            entries.push((ENTRY_SHAPELESS, body.freeze().to_vec()));
            index += 1;
        }

        for recipe in &manager.shaped {
            let (width, height, cells) = recipe.ingredient_map();
            let mut body = BinaryWriter::new();
            body.put_i32(width as i32);
            body.put_i32(height as i32);
            for cell in &cells {
                write_slot(&mut body, cell.as_ref());
            }
            body.put_i32(1);
            write_slot(&mut body, Some(&recipe.result));
            body.put_bytes(&recipe_uuid(index));
            entries.push((ENTRY_SHAPED, body.freeze().to_vec()));
            index += 1;
        }

        for recipe in &manager.furnace {
            let mut body = BinaryWriter::new();
            let exact = recipe.input.damage.filter(|damage| *damage != 0);
            let kind = match exact {
                Some(damage) => {
                    body.put_i32((recipe.input.id as i32) << 16 | damage as i32);
                    ENTRY_FURNACE_DATA
                }
                None => {
                    body.put_i32(recipe.input.id as i32);
                    ENTRY_FURNACE
                }
            };
            write_slot(&mut body, Some(&recipe.result));
            entries.push((kind, body.freeze().to_vec()));
        }

        let mut w = BinaryWriter::new();
        w.put_u8(CRAFTING_DATA);
        w.put_i32(entries.len() as i32);
        for (kind, body) in &entries {
            w.put_i32(*kind);
            w.put_i32(body.len() as i32);
            w.put_bytes(body);
        }
        w.put_u8(self.clean_recipes as u8);
        Bytes::from(w.freeze())
    }
}
