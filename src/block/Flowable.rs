// Flowable 照搬 Nukkit BlockFlowable
use crate::block::Block;

pub struct Flowable;
pub const INSTANCE: Flowable = Flowable;

impl Block for Flowable {
    fn id(&self) -> u8 { 255 } // Nukkit 抽象基类 不进注册表
    fn name(&self) -> &'static str { "Flowable" }
}
