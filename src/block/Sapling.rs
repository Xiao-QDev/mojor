// Sapling 照搬 Nukkit BlockSapling
use crate::block::Block;

pub struct Sapling;
pub const INSTANCE: Sapling = Sapling;

impl Block for Sapling {
    fn id(&self) -> u8 { 6 }
    fn name(&self) -> &'static str { "Sapling" }
}
