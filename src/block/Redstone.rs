// Redstone 照搬 Nukkit BlockRedstone
use crate::block::Block;

pub struct Redstone;
pub const INSTANCE: Redstone = Redstone;

impl Block for Redstone {
    fn id(&self) -> u8 { 152 }
    fn name(&self) -> &'static str { "Redstone" }
}
