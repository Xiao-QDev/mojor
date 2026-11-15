// Grass 照搬 Nukkit BlockGrass
use crate::block::Block;

pub struct Grass;
pub const INSTANCE: Grass = Grass;

impl Block for Grass {
    fn id(&self) -> u8 { 2 }
    fn name(&self) -> &'static str { "Grass" }
}
