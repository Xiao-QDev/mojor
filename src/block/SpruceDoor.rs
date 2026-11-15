// SpruceDoor 照搬 Nukkit BlockSpruceDoor
use crate::block::Block;

pub struct SpruceDoor;
pub const INSTANCE: SpruceDoor = SpruceDoor;

impl Block for SpruceDoor {
    fn id(&self) -> u8 { 193 }
    fn name(&self) -> &'static str { "SpruceDoor" }
}
