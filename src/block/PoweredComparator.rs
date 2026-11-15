// PoweredComparator 照搬 Nukkit BlockPoweredComparator
use crate::block::Block;

pub struct PoweredComparator;
pub const INSTANCE: PoweredComparator = PoweredComparator;

impl Block for PoweredComparator {
    fn id(&self) -> u8 { 150 }
    fn name(&self) -> &'static str { "PoweredComparator" }
}
