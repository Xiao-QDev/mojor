// Bricks 照搬 Nukkit BlockBricks
use crate::block::Block;

pub struct Bricks;
pub const INSTANCE: Bricks = Bricks;

impl Block for Bricks {
    fn id(&self) -> u8 { 45 }
    fn name(&self) -> &'static str { "Bricks" }
}
