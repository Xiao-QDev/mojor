// AcaciaDoor 照搬 Nukkit BlockAcaciaDoor
use crate::block::Block;

pub struct AcaciaDoor;
pub const INSTANCE: AcaciaDoor = AcaciaDoor;

impl Block for AcaciaDoor {
    fn id(&self) -> u8 { 196 }
    fn name(&self) -> &'static str { "AcaciaDoor" }
}
