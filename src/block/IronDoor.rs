// IronDoor 照搬 Nukkit BlockIronDoor
use crate::block::Block;

pub struct IronDoor;
pub const INSTANCE: IronDoor = IronDoor;

impl Block for IronDoor {
    fn id(&self) -> u8 { 71 }
    fn name(&self) -> &'static str { "IronDoor" }
}
