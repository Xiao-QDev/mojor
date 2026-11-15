// StonePressurePlate 照搬 Nukkit BlockStonePressurePlate
use crate::block::Block;

pub struct StonePressurePlate;
pub const INSTANCE: StonePressurePlate = StonePressurePlate;

impl Block for StonePressurePlate {
    fn id(&self) -> u8 { 70 }
    fn name(&self) -> &'static str { "StonePressurePlate" }
}
