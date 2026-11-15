// WoodStairs 照搬 Nukkit BlockWoodStairs
use crate::block::Block;

pub struct WoodStairs;
pub const INSTANCE: WoodStairs = WoodStairs;

impl Block for WoodStairs {
    fn id(&self) -> u8 { 53 }
    fn name(&self) -> &'static str { "WoodStairs" }
}
