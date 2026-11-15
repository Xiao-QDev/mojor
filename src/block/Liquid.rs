// Liquid 照搬 Nukkit BlockLiquid
use crate::block::Block;

pub struct Liquid;
pub const INSTANCE: Liquid = Liquid;

impl Block for Liquid {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Liquid" }
}
