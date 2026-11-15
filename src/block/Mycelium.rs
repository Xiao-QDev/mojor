// Mycelium 照搬 Nukkit BlockMycelium
use crate::block::Block;

pub struct Mycelium;
pub const INSTANCE: Mycelium = Mycelium;

impl Block for Mycelium {
    fn id(&self) -> u8 { 110 }
    fn name(&self) -> &'static str { "Mycelium" }
}
