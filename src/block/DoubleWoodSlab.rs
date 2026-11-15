// DoubleWoodSlab 照搬 Nukkit BlockDoubleWoodSlab
use crate::block::Block;

pub struct DoubleWoodSlab;
pub const INSTANCE: DoubleWoodSlab = DoubleWoodSlab;

impl Block for DoubleWoodSlab {
    fn id(&self) -> u8 { 157 }
    fn name(&self) -> &'static str { "DoubleWoodSlab" }
}
