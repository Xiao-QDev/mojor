// DarkOakWoodStairs 照搬 Nukkit BlockDarkOakWoodStairs
use crate::block::Block;

pub struct DarkOakWoodStairs;
pub const INSTANCE: DarkOakWoodStairs = DarkOakWoodStairs;

impl Block for DarkOakWoodStairs {
    fn id(&self) -> u8 { 164 }
    fn name(&self) -> &'static str { "DarkOakWoodStairs" }
}
