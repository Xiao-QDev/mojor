// DoubleSlab 照搬 Nukkit BlockDoubleSlab
use crate::block::Block;

pub struct DoubleSlab;
pub const INSTANCE: DoubleSlab = DoubleSlab;

impl Block for DoubleSlab {
    fn id(&self) -> u8 { 43 }
    fn name(&self) -> &'static str { "DoubleSlab" }
}
