// FlowerPot 照搬 Nukkit BlockFlowerPot
use crate::block::Block;

pub struct FlowerPot;
pub const INSTANCE: FlowerPot = FlowerPot;

impl Block for FlowerPot {
    fn id(&self) -> u8 { 140 }
    fn name(&self) -> &'static str { "FlowerPot" }
}
