// WallSign 照搬 Nukkit BlockWallSign
use crate::block::Block;

pub struct WallSign;
pub const INSTANCE: WallSign = WallSign;

impl Block for WallSign {
    fn id(&self) -> u8 { 68 }
    fn name(&self) -> &'static str { "WallSign" }
}
