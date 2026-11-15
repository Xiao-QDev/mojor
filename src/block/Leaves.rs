// Leaves 照搬 Nukkit BlockLeaves
use crate::block::Block;

pub struct Leaves;
pub const INSTANCE: Leaves = Leaves;

impl Block for Leaves {
    fn id(&self) -> u8 { 18 }
    fn name(&self) -> &'static str { "Leaves" }
}
