// NetherBrickFence 照搬 Nukkit BlockNetherBrickFence
use crate::block::Block;

pub struct NetherBrickFence;
pub const INSTANCE: NetherBrickFence = NetherBrickFence;

impl Block for NetherBrickFence {
    fn id(&self) -> u8 { 113 }
    fn name(&self) -> &'static str { "NetherBrickFence" }
}
