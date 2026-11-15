// SoulSand 照搬 Nukkit BlockSoulSand
use crate::block::Block;

pub struct SoulSand;
pub const INSTANCE: SoulSand = SoulSand;

impl Block for SoulSand {
    fn id(&self) -> u8 { 88 }
    fn name(&self) -> &'static str { "SoulSand" }
}
