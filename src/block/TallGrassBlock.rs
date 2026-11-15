// TallGrassBlock 照搬 Nukkit BlockTallGrassBlock
use crate::block::Block;

pub struct TallGrassBlock;
pub const INSTANCE: TallGrassBlock = TallGrassBlock;

impl Block for TallGrassBlock {
    fn id(&self) -> u8 { 31 }
    fn name(&self) -> &'static str { "TallGrassBlock" }
}
