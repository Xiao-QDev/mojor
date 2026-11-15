// Quartz 照搬 Nukkit BlockQuartz
use crate::block::Block;

pub struct Quartz;
pub const INSTANCE: Quartz = Quartz;

impl Block for Quartz {
    fn id(&self) -> u8 { 155 }
    fn name(&self) -> &'static str { "Quartz" }
}
