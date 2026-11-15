// TallGrass 照搬 Nukkit BlockTallGrass
use crate::block::Block;

pub struct TallGrass;
pub const INSTANCE: TallGrass = TallGrass;

impl Block for TallGrass {
    fn id(&self) -> u8 { 31 }
    fn name(&self) -> &'static str { "TallGrass" }
}
