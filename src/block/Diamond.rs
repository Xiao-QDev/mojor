// Diamond 照搬 Nukkit BlockDiamond
use crate::block::Block;

pub struct Diamond;
pub const INSTANCE: Diamond = Diamond;

impl Block for Diamond {
    fn id(&self) -> u8 { 57 }
    fn name(&self) -> &'static str { "Diamond" }
}
