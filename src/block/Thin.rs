// Thin 照搬 Nukkit BlockThin
use crate::block::Block;

pub struct Thin;
pub const INSTANCE: Thin = Thin;

impl Block for Thin {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Thin" }
}
