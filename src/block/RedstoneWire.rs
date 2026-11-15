// RedstoneWire 照搬 Nukkit BlockRedstoneWire
use crate::block::Block;

pub struct RedstoneWire;
pub const INSTANCE: RedstoneWire = RedstoneWire;

impl Block for RedstoneWire {
    fn id(&self) -> u8 { 55 }
    fn name(&self) -> &'static str { "RedstoneWire" }
}
