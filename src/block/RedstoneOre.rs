// RedstoneOre 照搬 Nukkit BlockRedstoneOre
use crate::block::Block;

pub struct RedstoneOre;
pub const INSTANCE: RedstoneOre = RedstoneOre;

impl Block for RedstoneOre {
    fn id(&self) -> u8 { 73 }
    fn name(&self) -> &'static str { "RedstoneOre" }
}
