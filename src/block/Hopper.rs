// Hopper 照搬 Nukkit BlockHopper
use crate::block::Block;

pub struct Hopper;
pub const INSTANCE: Hopper = Hopper;

impl Block for Hopper {
    fn id(&self) -> u8 { 154 }
    fn name(&self) -> &'static str { "Hopper" }
}
