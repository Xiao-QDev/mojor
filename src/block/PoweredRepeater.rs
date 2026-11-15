// PoweredRepeater 照搬 Nukkit BlockPoweredRepeater
use crate::block::Block;

pub struct PoweredRepeater;
pub const INSTANCE: PoweredRepeater = PoweredRepeater;

impl Block for PoweredRepeater {
    fn id(&self) -> u8 { 94 }
    fn name(&self) -> &'static str { "PoweredRepeater" }
}
