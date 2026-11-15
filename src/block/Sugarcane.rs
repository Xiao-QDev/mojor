// Sugarcane 照搬 Nukkit BlockSugarcane
use crate::block::Block;

pub struct Sugarcane;
pub const INSTANCE: Sugarcane = Sugarcane;

impl Block for Sugarcane {
    fn id(&self) -> u8 { 83 }
    fn name(&self) -> &'static str { "Sugarcane" }
}
