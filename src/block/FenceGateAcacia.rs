// FenceGateAcacia 照搬 Nukkit BlockFenceGateAcacia
use crate::block::Block;

pub struct FenceGateAcacia;
pub const INSTANCE: FenceGateAcacia = FenceGateAcacia;

impl Block for FenceGateAcacia {
    fn id(&self) -> u8 { 187 }
    fn name(&self) -> &'static str { "FenceGateAcacia" }
}
