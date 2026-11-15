// FlowerBlock 照搬 Nukkit BlockFlowerBlock
use crate::block::Block;

pub struct FlowerBlock;
pub const INSTANCE: FlowerBlock = FlowerBlock;

impl Block for FlowerBlock {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "FlowerBlock" }
}
