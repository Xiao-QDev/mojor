// Rail 照搬 Nukkit BlockRail
use crate::block::Block;

pub struct Rail;
pub const INSTANCE: Rail = Rail;

impl Block for Rail {
    fn id(&self) -> u8 { 66 }
    fn name(&self) -> &'static str { "Rail" }
}
