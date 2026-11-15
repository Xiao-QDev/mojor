// RedSandstone 照搬 Nukkit BlockRedSandstone
use crate::block::Block;

pub struct RedSandstone;
pub const INSTANCE: RedSandstone = RedSandstone;

impl Block for RedSandstone {
    fn id(&self) -> u8 { 179 }
    fn name(&self) -> &'static str { "RedSandstone" }
}
