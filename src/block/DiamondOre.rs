// DiamondOre 照搬 Nukkit BlockDiamondOre
use crate::block::Block;

pub struct DiamondOre;
pub const INSTANCE: DiamondOre = DiamondOre;

impl Block for DiamondOre {
    fn id(&self) -> u8 { 56 }
    fn name(&self) -> &'static str { "DiamondOre" }
}
