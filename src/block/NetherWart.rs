// NetherWart 照搬 Nukkit BlockNetherWart
use crate::block::Block;

pub struct NetherWart;
pub const INSTANCE: NetherWart = NetherWart;

impl Block for NetherWart {
    fn id(&self) -> u8 { 115 }
    fn name(&self) -> &'static str { "NetherWart" }
}
