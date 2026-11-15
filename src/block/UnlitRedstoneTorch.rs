// UnlitRedstoneTorch 照搬 Nukkit BlockUnlitRedstoneTorch
use crate::block::Block;

pub struct UnlitRedstoneTorch;
pub const INSTANCE: UnlitRedstoneTorch = UnlitRedstoneTorch;

impl Block for UnlitRedstoneTorch {
    fn id(&self) -> u8 { 75 }
    fn name(&self) -> &'static str { "UnlitRedstoneTorch" }
}
