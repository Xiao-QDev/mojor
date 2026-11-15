// WoodDoor 照搬 Nukkit BlockWoodDoor
use crate::block::Block;

pub struct WoodDoor;
pub const INSTANCE: WoodDoor = WoodDoor;

impl Block for WoodDoor {
    fn id(&self) -> u8 { 64 }
    fn name(&self) -> &'static str { "WoodDoor" }
}
