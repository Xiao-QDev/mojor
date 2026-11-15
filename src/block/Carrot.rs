// Carrot 照搬 Nukkit BlockCarrot
use crate::block::Block;

pub struct Carrot;
pub const INSTANCE: Carrot = Carrot;

impl Block for Carrot {
    fn id(&self) -> u8 { 141 }
    fn name(&self) -> &'static str { "Carrot" }
}
