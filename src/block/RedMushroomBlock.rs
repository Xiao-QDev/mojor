// RedMushroomBlock 照搬 Nukkit BlockRedMushroomBlock
use crate::block::Block;

pub struct RedMushroomBlock;
pub const INSTANCE: RedMushroomBlock = RedMushroomBlock;

impl Block for RedMushroomBlock {
    fn id(&self) -> u8 { 100 }
    fn name(&self) -> &'static str { "RedMushroomBlock" }
}
