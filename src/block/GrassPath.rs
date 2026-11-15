// GrassPath 照搬 Nukkit BlockGrassPath
use crate::block::Block;

pub struct GrassPath;
pub const INSTANCE: GrassPath = GrassPath;

impl Block for GrassPath {
    fn id(&self) -> u8 { 198 }
    fn name(&self) -> &'static str { "GrassPath" }
}
