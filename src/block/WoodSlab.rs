// WoodSlab 照搬 Nukkit BlockWoodSlab
use crate::block::Block;

pub struct WoodSlab;
pub const INSTANCE: WoodSlab = WoodSlab;

impl Block for WoodSlab {
    fn id(&self) -> u8 { 158 }
    fn name(&self) -> &'static str { "WoodSlab" }
}
