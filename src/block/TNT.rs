// TNT 照搬 Nukkit BlockTNT
use crate::block::Block;

pub struct TNT;
pub const INSTANCE: TNT = TNT;

impl Block for TNT {
    fn id(&self) -> u8 { 46 }
    fn name(&self) -> &'static str { "TNT" }
}
