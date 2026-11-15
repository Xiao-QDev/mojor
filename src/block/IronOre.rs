// IronOre 照搬 Nukkit BlockIronOre
use crate::block::Block;

pub struct IronOre;
pub const INSTANCE: IronOre = IronOre;

impl Block for IronOre {
    fn id(&self) -> u8 { 15 }
    fn name(&self) -> &'static str { "IronOre" }
}
