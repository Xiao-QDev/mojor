// Fence 照搬 Nukkit BlockFence
use crate::block::Block;

pub struct Fence;
pub const INSTANCE: Fence = Fence;

impl Block for Fence {
    fn id(&self) -> u8 { 85 }
    fn name(&self) -> &'static str { "Fence" }
}
