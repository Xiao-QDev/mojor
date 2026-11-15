// Fire 照搬 Nukkit BlockFire
use crate::block::Block;

pub struct Fire;
pub const INSTANCE: Fire = Fire;

impl Block for Fire {
    fn id(&self) -> u8 { 51 }
    fn name(&self) -> &'static str { "Fire" }
}
