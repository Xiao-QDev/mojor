// SandstoneStairs 照搬 Nukkit BlockSandstoneStairs
use crate::block::Block;

pub struct SandstoneStairs;
pub const INSTANCE: SandstoneStairs = SandstoneStairs;

impl Block for SandstoneStairs {
    fn id(&self) -> u8 { 128 }
    fn name(&self) -> &'static str { "SandstoneStairs" }
}
