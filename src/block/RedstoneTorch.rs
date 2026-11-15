// RedstoneTorch 照搬 Nukkit BlockRedstoneTorch
use crate::block::Block;

pub struct RedstoneTorch;
pub const INSTANCE: RedstoneTorch = RedstoneTorch;

impl Block for RedstoneTorch {
    fn id(&self) -> u8 { 76 }
    fn name(&self) -> &'static str { "RedstoneTorch" }
}
