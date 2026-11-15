// Planks 照搬 Nukkit BlockPlanks
use crate::block::Block;

pub struct Planks;
pub const INSTANCE: Planks = Planks;

impl Block for Planks {
    fn id(&self) -> u8 { 5 }
    fn name(&self) -> &'static str { "Planks" }
}
