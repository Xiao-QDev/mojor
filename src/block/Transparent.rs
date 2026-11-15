// Transparent 照搬 Nukkit BlockTransparent
use crate::block::Block;

pub struct Transparent;
pub const INSTANCE: Transparent = Transparent;

impl Block for Transparent {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Transparent" }
}
