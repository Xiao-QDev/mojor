// Lapis 照搬 Nukkit BlockLapis
use crate::block::Block;

pub struct Lapis;
pub const INSTANCE: Lapis = Lapis;

impl Block for Lapis {
    fn id(&self) -> u8 { 22 }
    fn name(&self) -> &'static str { "Lapis" }
}
