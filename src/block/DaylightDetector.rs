// DaylightDetector 照搬 Nukkit BlockDaylightDetector
use crate::block::Block;

pub struct DaylightDetector;
pub const INSTANCE: DaylightDetector = DaylightDetector;

impl Block for DaylightDetector {
    fn id(&self) -> u8 { 151 }
    fn name(&self) -> &'static str { "DaylightDetector" }
}
