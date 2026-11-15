// Melon 照搬 Nukkit BlockMelon
use crate::block::Block;

pub struct Melon;
pub const INSTANCE: Melon = Melon;

impl Block for Melon {
    fn id(&self) -> u8 { 103 }
    fn name(&self) -> &'static str { "Melon" }
}
