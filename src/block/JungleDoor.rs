// JungleDoor 照搬 Nukkit BlockJungleDoor
use crate::block::Block;

pub struct JungleDoor;
pub const INSTANCE: JungleDoor = JungleDoor;

impl Block for JungleDoor {
    fn id(&self) -> u8 { 195 }
    fn name(&self) -> &'static str { "JungleDoor" }
}
