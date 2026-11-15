// ItemFrame 照搬 Nukkit BlockItemFrame
use crate::block::Block;

pub struct ItemFrame;
pub const INSTANCE: ItemFrame = ItemFrame;

impl Block for ItemFrame {
    fn id(&self) -> u8 { 199 }
    fn name(&self) -> &'static str { "ItemFrame" }
}
