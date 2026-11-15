// RedstoneSource 照搬 Nukkit BlockRedstoneSource
use crate::block::Block;

pub struct RedstoneSource;
pub const INSTANCE: RedstoneSource = RedstoneSource;

impl Block for RedstoneSource {
    fn id(&self) -> u8 { 152 }
    fn name(&self) -> &'static str { "RedstoneSource" }
}
