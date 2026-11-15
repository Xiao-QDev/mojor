// Gravel 照搬 Nukkit BlockGravel
use crate::block::Block;

pub struct Gravel;
pub const INSTANCE: Gravel = Gravel;

impl Block for Gravel {
    fn id(&self) -> u8 { 13 }
    fn name(&self) -> &'static str { "Gravel" }
}
