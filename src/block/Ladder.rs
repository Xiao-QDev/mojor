// Ladder 照搬 Nukkit BlockLadder
use crate::block::Block;

pub struct Ladder;
pub const INSTANCE: Ladder = Ladder;

impl Block for Ladder {
    fn id(&self) -> u8 { 65 }
    fn name(&self) -> &'static str { "Ladder" }
}
