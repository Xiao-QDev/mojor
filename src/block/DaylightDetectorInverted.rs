// DaylightDetectorInverted 照搬 Nukkit BlockDaylightDetectorInverted
use crate::block::Block;

pub struct DaylightDetectorInverted;
pub const INSTANCE: DaylightDetectorInverted = DaylightDetectorInverted;

impl Block for DaylightDetectorInverted {
    fn id(&self) -> u8 { 178 }
    fn name(&self) -> &'static str { "DaylightDetectorInverted" }
}
