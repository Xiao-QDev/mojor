// Cactus 照搬 Nukkit BlockCactus
use crate::block::Block;

pub struct Cactus;
pub const INSTANCE: Cactus = Cactus;

impl Block for Cactus {
    fn id(&self) -> u8 { 81 }
    fn name(&self) -> &'static str { "Cactus" }
}
