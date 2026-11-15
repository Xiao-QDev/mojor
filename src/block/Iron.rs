// Iron 照搬 Nukkit BlockIron
use crate::block::Block;

pub struct Iron;
pub const INSTANCE: Iron = Iron;

impl Block for Iron {
    fn id(&self) -> u8 { 42 }
    fn name(&self) -> &'static str { "Iron" }
}
