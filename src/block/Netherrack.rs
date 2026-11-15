// Netherrack 照搬 Nukkit BlockNetherrack
use crate::block::Block;

pub struct Netherrack;
pub const INSTANCE: Netherrack = Netherrack;

impl Block for Netherrack {
    fn id(&self) -> u8 { 87 }
    fn name(&self) -> &'static str { "Netherrack" }
}
