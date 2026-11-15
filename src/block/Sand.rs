// Sand 照搬 Nukkit BlockSand
use crate::block::Block;

pub struct Sand;
pub const INSTANCE: Sand = Sand;

impl Block for Sand {
    fn id(&self) -> u8 { 12 }
    fn name(&self) -> &'static str { "Sand" }
}
