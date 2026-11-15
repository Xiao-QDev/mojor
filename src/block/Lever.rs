// Lever 照搬 Nukkit BlockLever
use crate::block::Block;

pub struct Lever;
pub const INSTANCE: Lever = Lever;

impl Block for Lever {
    fn id(&self) -> u8 { 69 }
    fn name(&self) -> &'static str { "Lever" }
}
