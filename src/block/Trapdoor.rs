// Trapdoor 照搬 Nukkit BlockTrapdoor
use crate::block::Block;

pub struct Trapdoor;
pub const INSTANCE: Trapdoor = Trapdoor;

impl Block for Trapdoor {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Trapdoor" }
}
