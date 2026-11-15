// Flower 照搬 Nukkit BlockFlower
use crate::block::Block;

pub struct Flower;
pub const INSTANCE: Flower = Flower;

impl Block for Flower {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Flower" }
}
