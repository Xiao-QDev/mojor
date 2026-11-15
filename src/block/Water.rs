// Water 照搬 Nukkit BlockWater
use crate::block::Block;

pub struct Water;
pub const INSTANCE: Water = Water;

impl Block for Water {
    fn id(&self) -> u8 { 8 }
    fn name(&self) -> &'static str { "Water" }
}
