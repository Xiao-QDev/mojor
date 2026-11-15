// Portal 照搬 Nukkit BlockPortal
use crate::block::Block;

pub struct Portal;
pub const INSTANCE: Portal = Portal;

impl Block for Portal {
    fn id(&self) -> u8 { 90 }
    fn name(&self) -> &'static str { "Portal" }
}
