// JungleWoodStairs 照搬 Nukkit BlockJungleWoodStairs
use crate::block::Block;

pub struct JungleWoodStairs;
pub const INSTANCE: JungleWoodStairs = JungleWoodStairs;

impl Block for JungleWoodStairs {
    fn id(&self) -> u8 { 136 }
    fn name(&self) -> &'static str { "JungleWoodStairs" }
}
