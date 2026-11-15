// NetherBrickStairs 照搬 Nukkit BlockNetherBrickStairs
use crate::block::Block;

pub struct NetherBrickStairs;
pub const INSTANCE: NetherBrickStairs = NetherBrickStairs;

impl Block for NetherBrickStairs {
    fn id(&self) -> u8 { 114 }
    fn name(&self) -> &'static str { "NetherBrickStairs" }
}
