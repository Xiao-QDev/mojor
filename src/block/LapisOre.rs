// LapisOre 照搬 Nukkit BlockLapisOre
use crate::block::Block;

pub struct LapisOre;
pub const INSTANCE: LapisOre = LapisOre;

impl Block for LapisOre {
    fn id(&self) -> u8 { 21 }
    fn name(&self) -> &'static str { "LapisOre" }
}
