// Air 照搬 Nukkit BlockAir
use crate::block::Block;

pub struct Air;
pub const INSTANCE: Air = Air;

impl Block for Air {
    fn id(&self) -> u8 { 0 }
    fn name(&self) -> &'static str { "Air" }
}
