// Cauldron 照搬 Nukkit BlockCauldron
use crate::block::Block;

pub struct Cauldron;
pub const INSTANCE: Cauldron = Cauldron;

impl Block for Cauldron {
    fn id(&self) -> u8 { 118 }
    fn name(&self) -> &'static str { "Cauldron" }
}
