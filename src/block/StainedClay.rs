// StainedClay 照搬 Nukkit BlockStainedClay
use crate::block::Block;

pub struct StainedClay;
pub const INSTANCE: StainedClay = StainedClay;

impl Block for StainedClay {
    fn id(&self) -> u8 { 159 }
    fn name(&self) -> &'static str { "StainedClay" }
}
