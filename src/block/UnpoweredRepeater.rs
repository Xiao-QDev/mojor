// UnpoweredRepeater 照搬 Nukkit BlockUnpoweredRepeater
use crate::block::Block;

pub struct UnpoweredRepeater;
pub const INSTANCE: UnpoweredRepeater = UnpoweredRepeater;

impl Block for UnpoweredRepeater {
    fn id(&self) -> u8 { 93 }
    fn name(&self) -> &'static str { "UnpoweredRepeater" }
}
