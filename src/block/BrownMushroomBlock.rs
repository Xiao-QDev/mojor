// BrownMushroomBlock 照搬 Nukkit BlockBrownMushroomBlock
use crate::block::Block;

pub struct BrownMushroomBlock;
pub const INSTANCE: BrownMushroomBlock = BrownMushroomBlock;

impl Block for BrownMushroomBlock {
    fn id(&self) -> u8 { 99 }
    fn name(&self) -> &'static str { "BrownMushroomBlock" }
}
