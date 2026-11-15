// RedSandstoneSlab 照搬 Nukkit BlockRedSandstoneSlab
use crate::block::Block;

pub struct RedSandstoneSlab;
pub const INSTANCE: RedSandstoneSlab = RedSandstoneSlab;

impl Block for RedSandstoneSlab {
    fn id(&self) -> u8 { 182 }
    fn name(&self) -> &'static str { "RedSandstoneSlab" }
}
