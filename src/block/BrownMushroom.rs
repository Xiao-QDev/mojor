// BrownMushroom 照搬 Nukkit BlockBrownMushroom
use crate::block::Block;

pub struct BrownMushroom;
pub const INSTANCE: BrownMushroom = BrownMushroom;

impl Block for BrownMushroom {
    fn id(&self) -> u8 { 39 }
    fn name(&self) -> &'static str { "BrownMushroom" }
}
