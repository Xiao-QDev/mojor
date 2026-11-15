// Cobweb 照搬 Nukkit BlockCobweb
use crate::block::Block;

pub struct Cobweb;
pub const INSTANCE: Cobweb = Cobweb;

impl Block for Cobweb {
    fn id(&self) -> u8 { 30 }
    fn name(&self) -> &'static str { "Cobweb" }
}
