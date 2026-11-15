// Bed 照搬 Nukkit BlockBed
use crate::block::Block;

pub struct Bed;
pub const INSTANCE: Bed = Bed;

impl Block for Bed {
    fn id(&self) -> u8 { 26 }
    fn name(&self) -> &'static str { "Bed" }
}
