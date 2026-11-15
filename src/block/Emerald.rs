// Emerald 照搬 Nukkit BlockEmerald
use crate::block::Block;

pub struct Emerald;
pub const INSTANCE: Emerald = Emerald;

impl Block for Emerald {
    fn id(&self) -> u8 { 133 }
    fn name(&self) -> &'static str { "Emerald" }
}
