// GlowingObsidian 照搬 Nukkit BlockGlowingObsidian
use crate::block::Block;

pub struct GlowingObsidian;
pub const INSTANCE: GlowingObsidian = GlowingObsidian;

impl Block for GlowingObsidian {
    fn id(&self) -> u8 { 246 }
    fn name(&self) -> &'static str { "GlowingObsidian" }
}
