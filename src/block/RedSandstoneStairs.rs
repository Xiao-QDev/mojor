// RedSandstoneStairs 照搬 Nukkit BlockRedSandstoneStairs
use crate::block::Block;

pub struct RedSandstoneStairs;
pub const INSTANCE: RedSandstoneStairs = RedSandstoneStairs;

impl Block for RedSandstoneStairs {
    fn id(&self) -> u8 { 180 }
    fn name(&self) -> &'static str { "RedSandstoneStairs" }
}
