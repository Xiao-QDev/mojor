// Clay 照搬 Nukkit BlockClay
use crate::block::Block;

pub struct Clay;
pub const INSTANCE: Clay = Clay;

impl Block for Clay {
    fn id(&self) -> u8 { 82 }
    fn name(&self) -> &'static str { "Clay" }
}
