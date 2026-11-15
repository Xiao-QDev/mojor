// Leaves2 照搬 Nukkit BlockLeaves2
use crate::block::Block;

pub struct Leaves2;
pub const INSTANCE: Leaves2 = Leaves2;

impl Block for Leaves2 {
    fn id(&self) -> u8 { 161 }
    fn name(&self) -> &'static str { "Leaves2" }
}
