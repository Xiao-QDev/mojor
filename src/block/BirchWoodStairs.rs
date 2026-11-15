// BirchWoodStairs 照搬 Nukkit BlockBirchWoodStairs
use crate::block::Block;

pub struct BirchWoodStairs;
pub const INSTANCE: BirchWoodStairs = BirchWoodStairs;

impl Block for BirchWoodStairs {
    fn id(&self) -> u8 { 135 }
    fn name(&self) -> &'static str { "BirchWoodStairs" }
}
