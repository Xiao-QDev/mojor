// Cobblestone 照搬 Nukkit BlockCobblestone
use crate::block::Block;

pub struct Cobblestone;
pub const INSTANCE: Cobblestone = Cobblestone;

impl Block for Cobblestone {
    fn id(&self) -> u8 { 4 }
    fn name(&self) -> &'static str { "Cobblestone" }
}
