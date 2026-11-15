// RedMushroom 照搬 Nukkit BlockRedMushroom
use crate::block::Block;

pub struct RedMushroom;
pub const INSTANCE: RedMushroom = RedMushroom;

impl Block for RedMushroom {
    fn id(&self) -> u8 { 40 }
    fn name(&self) -> &'static str { "RedMushroom" }
}
