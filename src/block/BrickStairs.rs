// BrickStairs 照搬 Nukkit BlockBrickStairs
use crate::block::Block;

pub struct BrickStairs;
pub const INSTANCE: BrickStairs = BrickStairs;

impl Block for BrickStairs {
    fn id(&self) -> u8 { 108 }
    fn name(&self) -> &'static str { "BrickStairs" }
}
