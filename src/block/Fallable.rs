// Fallable 照搬 Nukkit BlockFallable
use crate::block::Block;

pub struct Fallable;
pub const INSTANCE: Fallable = Fallable;

impl Block for Fallable {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Fallable" }
}
