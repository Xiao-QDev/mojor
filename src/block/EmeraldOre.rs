// EmeraldOre 照搬 Nukkit BlockEmeraldOre
use crate::block::Block;

pub struct EmeraldOre;
pub const INSTANCE: EmeraldOre = EmeraldOre;

impl Block for EmeraldOre {
    fn id(&self) -> u8 { 129 }
    fn name(&self) -> &'static str { "EmeraldOre" }
}
