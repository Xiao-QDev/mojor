// HeavyWeightedPressurePlate 照搬 Nukkit BlockHeavyWeightedPressurePlate
use crate::block::Block;

pub struct HeavyWeightedPressurePlate;
pub const INSTANCE: HeavyWeightedPressurePlate = HeavyWeightedPressurePlate;

impl Block for HeavyWeightedPressurePlate {
    fn id(&self) -> u8 { 148 }
    fn name(&self) -> &'static str { "HeavyWeightedPressurePlate" }
}
