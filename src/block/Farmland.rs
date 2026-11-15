// Farmland 照搬 Nukkit BlockFarmland
use crate::block::Block;

pub struct Farmland;
pub const INSTANCE: Farmland = Farmland;

impl Block for Farmland {
    fn id(&self) -> u8 { 60 }
    fn name(&self) -> &'static str { "Farmland" }
}
