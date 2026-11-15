// AcaciaWoodStairs 照搬 Nukkit BlockAcaciaWoodStairs
use crate::block::Block;

pub struct AcaciaWoodStairs;
pub const INSTANCE: AcaciaWoodStairs = AcaciaWoodStairs;

impl Block for AcaciaWoodStairs {
    fn id(&self) -> u8 { 163 }
    fn name(&self) -> &'static str { "AcaciaWoodStairs" }
}
