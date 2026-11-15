// MossStone 照搬 Nukkit BlockMossStone
use crate::block::Block;

pub struct MossStone;
pub const INSTANCE: MossStone = MossStone;

impl Block for MossStone {
    fn id(&self) -> u8 { 48 }
    fn name(&self) -> &'static str { "MossStone" }
}
