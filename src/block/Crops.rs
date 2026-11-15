// Crops 照搬 Nukkit BlockCrops
use crate::block::Block;

pub struct Crops;
pub const INSTANCE: Crops = Crops;

impl Block for Crops {
    fn id(&self) -> u8 { 59 }
    fn name(&self) -> &'static str { "Crops" }
}
