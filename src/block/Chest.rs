// Chest 照搬 Nukkit BlockChest
use crate::block::Block;

pub struct Chest;
pub const INSTANCE: Chest = Chest;

impl Block for Chest {
    fn id(&self) -> u8 { 54 }
    fn name(&self) -> &'static str { "Chest" }
}
