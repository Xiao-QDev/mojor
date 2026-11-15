// SnowLayer 照搬 Nukkit BlockSnowLayer
use crate::block::Block;

pub struct SnowLayer;
pub const INSTANCE: SnowLayer = SnowLayer;

impl Block for SnowLayer {
    fn id(&self) -> u8 { 78 }
    fn name(&self) -> &'static str { "SnowLayer" }
}
