// Gold 照搬 Nukkit BlockGold
use crate::block::Block;

pub struct Gold;
pub const INSTANCE: Gold = Gold;

impl Block for Gold {
    fn id(&self) -> u8 { 41 }
    fn name(&self) -> &'static str { "Gold" }
}
