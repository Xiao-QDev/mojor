// Cake 照搬 Nukkit BlockCake
use crate::block::Block;

pub struct Cake;
pub const INSTANCE: Cake = Cake;

impl Block for Cake {
    fn id(&self) -> u8 { 92 }
    fn name(&self) -> &'static str { "Cake" }
}
