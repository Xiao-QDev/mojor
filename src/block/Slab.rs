// Slab 照搬 Nukkit BlockSlab
use crate::block::Block;

pub struct Slab;
pub const INSTANCE: Slab = Slab;

impl Block for Slab {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Slab" }
}
