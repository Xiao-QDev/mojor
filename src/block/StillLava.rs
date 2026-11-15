// StillLava 照搬 Nukkit BlockStillLava
use crate::block::Block;

pub struct StillLava;
pub const INSTANCE: StillLava = StillLava;

impl Block for StillLava {
    fn id(&self) -> u8 { 11 }
    fn name(&self) -> &'static str { "StillLava" }
}
