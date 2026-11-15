// HardenedClay 照搬 Nukkit BlockHardenedClay
use crate::block::Block;

pub struct HardenedClay;
pub const INSTANCE: HardenedClay = HardenedClay;

impl Block for HardenedClay {
    fn id(&self) -> u8 { 172 }
    fn name(&self) -> &'static str { "HardenedClay" }
}
