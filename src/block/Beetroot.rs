// Beetroot 照搬 Nukkit BlockBeetroot
use crate::block::Block;

pub struct Beetroot;
pub const INSTANCE: Beetroot = Beetroot;

impl Block for Beetroot {
    fn id(&self) -> u8 { 244 }
    fn name(&self) -> &'static str { "Beetroot" }
}
