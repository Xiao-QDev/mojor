// PoweredRail 照搬 Nukkit BlockPoweredRail
use crate::block::Block;

pub struct PoweredRail;
pub const INSTANCE: PoweredRail = PoweredRail;

impl Block for PoweredRail {
    fn id(&self) -> u8 { 27 }
    fn name(&self) -> &'static str { "PoweredRail" }
}
