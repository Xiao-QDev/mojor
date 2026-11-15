// Door 照搬 Nukkit BlockDoor
use crate::block::Block;

pub struct Door;
pub const INSTANCE: Door = Door;

impl Block for Door {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Door" }
}
