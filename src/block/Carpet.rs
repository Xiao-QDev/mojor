// Carpet 照搬 Nukkit BlockCarpet
use crate::block::Block;

pub struct Carpet;
pub const INSTANCE: Carpet = Carpet;

impl Block for Carpet {
    fn id(&self) -> u8 { 171 }
    fn name(&self) -> &'static str { "Carpet" }
}
