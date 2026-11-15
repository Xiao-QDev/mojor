// FenceGate 照搬 Nukkit BlockFenceGate
use crate::block::Block;

pub struct FenceGate;
pub const INSTANCE: FenceGate = FenceGate;

impl Block for FenceGate {
    fn id(&self) -> u8 { 107 }
    fn name(&self) -> &'static str { "FenceGate" }
}
