// PackedIce 照搬 Nukkit BlockPackedIce
use crate::block::Block;

pub struct PackedIce;
pub const INSTANCE: PackedIce = PackedIce;

impl Block for PackedIce {
    fn id(&self) -> u8 { 174 }
    fn name(&self) -> &'static str { "PackedIce" }
}
