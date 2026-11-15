// LightWeightedPressurePlate 照搬 Nukkit BlockLightWeightedPressurePlate
use crate::block::Block;

pub struct LightWeightedPressurePlate;
pub const INSTANCE: LightWeightedPressurePlate = LightWeightedPressurePlate;

impl Block for LightWeightedPressurePlate {
    fn id(&self) -> u8 { 147 }
    fn name(&self) -> &'static str { "LightWeightedPressurePlate" }
}
