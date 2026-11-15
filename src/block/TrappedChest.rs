// TrappedChest 照搬 Nukkit BlockTrappedChest
use crate::block::Block;

pub struct TrappedChest;
pub const INSTANCE: TrappedChest = TrappedChest;

impl Block for TrappedChest {
    fn id(&self) -> u8 { 146 }
    fn name(&self) -> &'static str { "TrappedChest" }
}
