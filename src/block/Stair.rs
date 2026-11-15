// Stair 照搬 Nukkit BlockStair
use crate::block::Block;

pub struct Stair;
pub const INSTANCE: Stair = Stair;

impl Block for Stair {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Stair" }
}
