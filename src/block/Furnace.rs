// Furnace 照搬 Nukkit BlockFurnace
use crate::block::Block;

pub struct Furnace;
pub const INSTANCE: Furnace = Furnace;

impl Block for Furnace {
    fn id(&self) -> u8 { 61 }
    fn name(&self) -> &'static str { "Furnace" }
}
