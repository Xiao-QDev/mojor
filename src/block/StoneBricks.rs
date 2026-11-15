// StoneBricks 照搬 Nukkit BlockStoneBricks
use crate::block::Block;

pub struct StoneBricks;
pub const INSTANCE: StoneBricks = StoneBricks;

impl Block for StoneBricks {
    fn id(&self) -> u8 { 98 }
    fn name(&self) -> &'static str { "StoneBricks" }
}
