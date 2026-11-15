// BrewingStand 照搬 Nukkit BlockBrewingStand
use crate::block::Block;

pub struct BrewingStand;
pub const INSTANCE: BrewingStand = BrewingStand;

impl Block for BrewingStand {
    fn id(&self) -> u8 { 117 }
    fn name(&self) -> &'static str { "BrewingStand" }
}
