// DeadBush 照搬 Nukkit BlockDeadBush
use crate::block::Block;

pub struct DeadBush;
pub const INSTANCE: DeadBush = DeadBush;

impl Block for DeadBush {
    fn id(&self) -> u8 { 32 }
    fn name(&self) -> &'static str { "DeadBush" }
}
