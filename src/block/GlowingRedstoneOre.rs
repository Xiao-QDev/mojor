// GlowingRedstoneOre 照搬 Nukkit BlockGlowingRedstoneOre
use crate::block::Block;

pub struct GlowingRedstoneOre;
pub const INSTANCE: GlowingRedstoneOre = GlowingRedstoneOre;

impl Block for GlowingRedstoneOre {
    fn id(&self) -> u8 { 74 }
    fn name(&self) -> &'static str { "GlowingRedstoneOre" }
}
