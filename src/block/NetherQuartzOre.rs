// NetherQuartzOre 照搬 Nukkit BlockNetherQuartzOre
use crate::block::Block;

pub struct NetherQuartzOre;
pub const INSTANCE: NetherQuartzOre = NetherQuartzOre;

impl Block for NetherQuartzOre {
    fn id(&self) -> u8 { 153 }
    fn name(&self) -> &'static str { "NetherQuartzOre" }
}
