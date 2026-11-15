// NetherBrick 照搬 Nukkit BlockNetherBrick
use crate::block::Block;

pub struct NetherBrick;
pub const INSTANCE: NetherBrick = NetherBrick;

impl Block for NetherBrick {
    fn id(&self) -> u8 { 112 }
    fn name(&self) -> &'static str { "NetherBrick" }
}
