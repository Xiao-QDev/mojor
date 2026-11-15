// NetherReactor 照搬 Nukkit BlockNetherReactor
use crate::block::Block;

pub struct NetherReactor;
pub const INSTANCE: NetherReactor = NetherReactor;

impl Block for NetherReactor {
    fn id(&self) -> u8 { 247 }
    fn name(&self) -> &'static str { "NetherReactor" }
}
