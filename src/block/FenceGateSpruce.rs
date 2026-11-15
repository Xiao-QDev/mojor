// FenceGateSpruce 照搬 Nukkit BlockFenceGateSpruce
use crate::block::Block;

pub struct FenceGateSpruce;
pub const INSTANCE: FenceGateSpruce = FenceGateSpruce;

impl Block for FenceGateSpruce {
    fn id(&self) -> u8 { 183 }
    fn name(&self) -> &'static str { "FenceGateSpruce" }
}
