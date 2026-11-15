// SpruceWoodStairs 照搬 Nukkit BlockSpruceWoodStairs
use crate::block::Block;

pub struct SpruceWoodStairs;
pub const INSTANCE: SpruceWoodStairs = SpruceWoodStairs;

impl Block for SpruceWoodStairs {
    fn id(&self) -> u8 { 134 }
    fn name(&self) -> &'static str { "SpruceWoodStairs" }
}
