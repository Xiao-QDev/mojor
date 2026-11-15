// DoubleRedSandstoneSlab 照搬 Nukkit BlockDoubleRedSandstoneSlab
use crate::block::Block;

pub struct DoubleRedSandstoneSlab;
pub const INSTANCE: DoubleRedSandstoneSlab = DoubleRedSandstoneSlab;

impl Block for DoubleRedSandstoneSlab {
    fn id(&self) -> u8 { 181 }
    fn name(&self) -> &'static str { "DoubleRedSandstoneSlab" }
}
