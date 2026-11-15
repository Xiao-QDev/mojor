// UnpoweredComparator 照搬 Nukkit BlockUnpoweredComparator
use crate::block::Block;

pub struct UnpoweredComparator;
pub const INSTANCE: UnpoweredComparator = UnpoweredComparator;

impl Block for UnpoweredComparator {
    fn id(&self) -> u8 { 149 }
    fn name(&self) -> &'static str { "UnpoweredComparator" }
}
