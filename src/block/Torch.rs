// Torch 照搬 Nukkit BlockTorch
use crate::block::Block;

pub struct Torch;
pub const INSTANCE: Torch = Torch;

impl Block for Torch {
    fn id(&self) -> u8 { 50 }
    fn name(&self) -> &'static str { "Torch" }
}
