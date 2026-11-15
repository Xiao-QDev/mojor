// Dandelion 照搬 Nukkit BlockDandelion
use crate::block::Block;

pub struct Dandelion;
pub const INSTANCE: Dandelion = Dandelion;

impl Block for Dandelion {
    fn id(&self) -> u8 { 37 }
    fn name(&self) -> &'static str { "Dandelion" }
}
