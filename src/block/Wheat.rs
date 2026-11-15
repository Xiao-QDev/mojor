// Wheat 照搬 Nukkit BlockWheat
use crate::block::Block;

pub struct Wheat;
pub const INSTANCE: Wheat = Wheat;

impl Block for Wheat {
    fn id(&self) -> u8 { 59 }
    fn name(&self) -> &'static str { "Wheat" }
}
