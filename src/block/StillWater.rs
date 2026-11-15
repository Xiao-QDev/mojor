// StillWater 照搬 Nukkit BlockStillWater
use crate::block::Block;

pub struct StillWater;
pub const INSTANCE: StillWater = StillWater;

impl Block for StillWater {
    fn id(&self) -> u8 { 9 }
    fn name(&self) -> &'static str { "StillWater" }
}
