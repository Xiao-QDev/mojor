// Pumpkin 照搬 Nukkit BlockPumpkin
use crate::block::Block;

pub struct Pumpkin;
pub const INSTANCE: Pumpkin = Pumpkin;

impl Block for Pumpkin {
    fn id(&self) -> u8 { 86 }
    fn name(&self) -> &'static str { "Pumpkin" }
}
