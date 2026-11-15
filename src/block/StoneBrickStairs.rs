// StoneBrickStairs 照搬 Nukkit BlockStoneBrickStairs
use crate::block::Block;

pub struct StoneBrickStairs;
pub const INSTANCE: StoneBrickStairs = StoneBrickStairs;

impl Block for StoneBrickStairs {
    fn id(&self) -> u8 { 109 }
    fn name(&self) -> &'static str { "StoneBrickStairs" }
}
