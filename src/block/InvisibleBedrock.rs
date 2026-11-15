// InvisibleBedrock 照搬 Nukkit BlockInvisibleBedrock
use crate::block::Block;

pub struct InvisibleBedrock;
pub const INSTANCE: InvisibleBedrock = InvisibleBedrock;

impl Block for InvisibleBedrock {
    fn id(&self) -> u8 { 95 }
    fn name(&self) -> &'static str { "InvisibleBedrock" }
}
