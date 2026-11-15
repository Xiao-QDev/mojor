// Sandstone 照搬 Nukkit BlockSandstone
use crate::block::Block;

pub struct Sandstone;
pub const INSTANCE: Sandstone = Sandstone;

impl Block for Sandstone {
    fn id(&self) -> u8 { 24 }
    fn name(&self) -> &'static str { "Sandstone" }
}
