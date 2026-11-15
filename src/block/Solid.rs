// Solid 照搬 Nukkit BlockSolid
use crate::block::Block;

pub struct Solid;
pub const INSTANCE: Solid = Solid;

impl Block for Solid {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Solid" }
}
