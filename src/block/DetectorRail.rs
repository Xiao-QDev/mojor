// DetectorRail 照搬 Nukkit BlockDetectorRail
use crate::block::Block;

pub struct DetectorRail;
pub const INSTANCE: DetectorRail = DetectorRail;

impl Block for DetectorRail {
    fn id(&self) -> u8 { 28 }
    fn name(&self) -> &'static str { "DetectorRail" }
}
