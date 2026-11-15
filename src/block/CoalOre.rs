// CoalOre 照搬 Nukkit BlockCoalOre
use crate::block::Block;

pub struct CoalOre;
pub const INSTANCE: CoalOre = CoalOre;

impl Block for CoalOre {
    fn id(&self) -> u8 { 16 }
    fn name(&self) -> &'static str { "CoalOre" }
}
