// Vine 照搬 Nukkit BlockVine
use crate::block::Block;

pub struct Vine;
pub const INSTANCE: Vine = Vine;

impl Block for Vine {
    fn id(&self) -> u8 { 106 }
    fn name(&self) -> &'static str { "Vine" }
}
