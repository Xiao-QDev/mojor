// FenceGateDarkOak 照搬 Nukkit BlockFenceGateDarkOak
use crate::block::Block;

pub struct FenceGateDarkOak;
pub const INSTANCE: FenceGateDarkOak = FenceGateDarkOak;

impl Block for FenceGateDarkOak {
    fn id(&self) -> u8 { 186 }
    fn name(&self) -> &'static str { "FenceGateDarkOak" }
}
