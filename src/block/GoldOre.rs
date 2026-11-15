// GoldOre 照搬 Nukkit BlockGoldOre
use crate::block::Block;

pub struct GoldOre;
pub const INSTANCE: GoldOre = GoldOre;

impl Block for GoldOre {
    fn id(&self) -> u8 { 14 }
    fn name(&self) -> &'static str { "GoldOre" }
}
