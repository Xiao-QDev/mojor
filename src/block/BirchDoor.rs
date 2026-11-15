// BirchDoor 照搬 Nukkit BlockBirchDoor
use crate::block::Block;

pub struct BirchDoor;
pub const INSTANCE: BirchDoor = BirchDoor;

impl Block for BirchDoor {
    fn id(&self) -> u8 { 194 }
    fn name(&self) -> &'static str { "BirchDoor" }
}
