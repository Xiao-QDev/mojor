// CobblestoneStairs 照搬 Nukkit BlockCobblestoneStairs
use crate::block::Block;

pub struct CobblestoneStairs;
pub const INSTANCE: CobblestoneStairs = CobblestoneStairs;

impl Block for CobblestoneStairs {
    fn id(&self) -> u8 { 67 }
    fn name(&self) -> &'static str { "CobblestoneStairs" }
}
