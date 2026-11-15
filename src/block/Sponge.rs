// Sponge 照搬 Nukkit BlockSponge
use crate::block::Block;

pub struct Sponge;
pub const INSTANCE: Sponge = Sponge;

impl Block for Sponge {
    fn id(&self) -> u8 { 19 }
    fn name(&self) -> &'static str { "Sponge" }
}
