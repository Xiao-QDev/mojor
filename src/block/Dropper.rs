// Dropper 照搬 Nukkit BlockDropper
use crate::block::Block;

pub struct Dropper;
pub const INSTANCE: Dropper = Dropper;

impl Block for Dropper {
    fn id(&self) -> u8 { 125 }
    fn name(&self) -> &'static str { "Dropper" }
}
