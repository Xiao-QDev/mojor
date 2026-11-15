// SkullBlock 照搬 Nukkit BlockSkullBlock
use crate::block::Block;

pub struct SkullBlock;
pub const INSTANCE: SkullBlock = SkullBlock;

impl Block for SkullBlock {
    fn id(&self) -> u8 { 144 }
    fn name(&self) -> &'static str { "SkullBlock" }
}
