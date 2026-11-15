// EndPortalFrame 照搬 Nukkit BlockEndPortalFrame
use crate::block::Block;

pub struct EndPortalFrame;
pub const INSTANCE: EndPortalFrame = EndPortalFrame;

impl Block for EndPortalFrame {
    fn id(&self) -> u8 { 120 }
    fn name(&self) -> &'static str { "EndPortalFrame" }
}
