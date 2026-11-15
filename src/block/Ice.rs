// Ice 照搬 Nukkit BlockIce
use crate::block::Block;

pub struct Ice;
pub const INSTANCE: Ice = Ice;

impl Block for Ice {
    fn id(&self) -> u8 { 79 }
    fn name(&self) -> &'static str { "Ice" }
}
