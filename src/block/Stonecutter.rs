// Stonecutter 照搬 Nukkit BlockStonecutter
use crate::block::Block;

pub struct Stonecutter;
pub const INSTANCE: Stonecutter = Stonecutter;

impl Block for Stonecutter {
    fn id(&self) -> u8 { 245 }
    fn name(&self) -> &'static str { "Stonecutter" }
}
