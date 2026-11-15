// Glowstone 照搬 Nukkit BlockGlowstone
use crate::block::Block;

pub struct Glowstone;
pub const INSTANCE: Glowstone = Glowstone;

impl Block for Glowstone {
    fn id(&self) -> u8 { 89 }
    fn name(&self) -> &'static str { "Glowstone" }
}
