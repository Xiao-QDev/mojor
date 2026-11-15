// Dirt 照搬 Nukkit BlockDirt
use crate::block::Block;

pub struct Dirt;
pub const INSTANCE: Dirt = Dirt;

impl Block for Dirt {
    fn id(&self) -> u8 { 3 }
    fn name(&self) -> &'static str { "Dirt" }
}
