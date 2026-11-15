// EndStone 照搬 Nukkit BlockEndStone
use crate::block::Block;

pub struct EndStone;
pub const INSTANCE: EndStone = EndStone;

impl Block for EndStone {
    fn id(&self) -> u8 { 121 }
    fn name(&self) -> &'static str { "EndStone" }
}
