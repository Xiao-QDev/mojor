// FenceGateBirch 照搬 Nukkit BlockFenceGateBirch
use crate::block::Block;

pub struct FenceGateBirch;
pub const INSTANCE: FenceGateBirch = FenceGateBirch;

impl Block for FenceGateBirch {
    fn id(&self) -> u8 { 184 }
    fn name(&self) -> &'static str { "FenceGateBirch" }
}
