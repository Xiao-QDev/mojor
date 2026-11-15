// DarkOakDoor 照搬 Nukkit BlockDarkOakDoor
use crate::block::Block;

pub struct DarkOakDoor;
pub const INSTANCE: DarkOakDoor = DarkOakDoor;

impl Block for DarkOakDoor {
    fn id(&self) -> u8 { 197 }
    fn name(&self) -> &'static str { "DarkOakDoor" }
}
