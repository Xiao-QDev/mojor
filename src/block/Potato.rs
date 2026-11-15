// Potato 照搬 Nukkit BlockPotato
use crate::block::Block;

pub struct Potato;
pub const INSTANCE: Potato = Potato;

impl Block for Potato {
    fn id(&self) -> u8 { 142 }
    fn name(&self) -> &'static str { "Potato" }
}
