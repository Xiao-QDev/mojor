// Stone 照搬 Nukkit BlockStone
use crate::block::Block;

pub struct Stone;
pub const INSTANCE: Stone = Stone;

impl Block for Stone {
    fn id(&self) -> u8 { 1 }
    fn name(&self) -> &'static str { "Stone" }
}
