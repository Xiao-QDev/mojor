// Lava 照搬 Nukkit BlockLava
use crate::block::Block;

pub struct Lava;
pub const INSTANCE: Lava = Lava;

impl Block for Lava {
    fn id(&self) -> u8 { 10 }
    fn name(&self) -> &'static str { "Lava" }
}
