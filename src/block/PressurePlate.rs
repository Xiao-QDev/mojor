// PressurePlate 照搬 Nukkit BlockPressurePlate
use crate::block::Block;

pub struct PressurePlate;
pub const INSTANCE: PressurePlate = PressurePlate;

impl Block for PressurePlate {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "PressurePlate" }
}
