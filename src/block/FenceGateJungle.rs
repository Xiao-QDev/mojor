// FenceGateJungle 照搬 Nukkit BlockFenceGateJungle
use crate::block::Block;

pub struct FenceGateJungle;
pub const INSTANCE: FenceGateJungle = FenceGateJungle;

impl Block for FenceGateJungle {
    fn id(&self) -> u8 { 185 }
    fn name(&self) -> &'static str { "FenceGateJungle" }
}
