// Podzol 照搬 Nukkit BlockPodzol
use crate::block::Block;

pub struct Podzol;
pub const INSTANCE: Podzol = Podzol;

impl Block for Podzol {
    fn id(&self) -> u8 { 243 }
    fn name(&self) -> &'static str { "Podzol" }
}
