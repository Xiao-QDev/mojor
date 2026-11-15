// Dispenser 照搬 Nukkit BlockDispenser
use crate::block::Block;

pub struct Dispenser;
pub const INSTANCE: Dispenser = Dispenser;

impl Block for Dispenser {
    fn id(&self) -> u8 { 23 }
    fn name(&self) -> &'static str { "Dispenser" }
}
