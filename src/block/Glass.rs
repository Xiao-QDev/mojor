// Glass 照搬 Nukkit BlockGlass
use crate::block::Block;

pub struct Glass;
pub const INSTANCE: Glass = Glass;

impl Block for Glass {
    fn id(&self) -> u8 { 20 }
    fn name(&self) -> &'static str { "Glass" }
}
