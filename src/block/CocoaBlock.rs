// CocoaBlock 照搬 Nukkit BlockCocoaBlock
use crate::block::Block;

pub struct CocoaBlock;
pub const INSTANCE: CocoaBlock = CocoaBlock;

impl Block for CocoaBlock {
    fn id(&self) -> u8 { 127 }
    fn name(&self) -> &'static str { "CocoaBlock" }
}
