// Wool 照搬 Nukkit BlockWool
use crate::block::Block;

pub struct Wool;
pub const INSTANCE: Wool = Wool;

impl Block for Wool {
    fn id(&self) -> u8 { 35 }
    fn name(&self) -> &'static str { "Wool" }
}
