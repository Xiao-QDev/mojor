// WoodenButton 照搬 Nukkit BlockWoodenButton
use crate::block::Block;

pub struct WoodenButton;
pub const INSTANCE: WoodenButton = WoodenButton;

impl Block for WoodenButton {
    fn id(&self) -> u8 { 143 }
    fn name(&self) -> &'static str { "WoodenButton" }
}
