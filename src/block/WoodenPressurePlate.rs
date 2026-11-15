// WoodenPressurePlate 照搬 Nukkit BlockWoodenPressurePlate
use crate::block::Block;

pub struct WoodenPressurePlate;
pub const INSTANCE: WoodenPressurePlate = WoodenPressurePlate;

impl Block for WoodenPressurePlate {
    fn id(&self) -> u8 { 72 }
    fn name(&self) -> &'static str { "WoodenPressurePlate" }
}
