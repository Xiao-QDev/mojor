// StoneButton 照搬 Nukkit BlockStoneButton
use crate::block::Block;

pub struct StoneButton;
pub const INSTANCE: StoneButton = StoneButton;

impl Block for StoneButton {
    fn id(&self) -> u8 { 77 }
    fn name(&self) -> &'static str { "StoneButton" }
}
