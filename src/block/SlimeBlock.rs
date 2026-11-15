// SlimeBlock 照搬 Nukkit BlockSlimeBlock
use crate::block::Block;

pub struct SlimeBlock;
pub const INSTANCE: SlimeBlock = SlimeBlock;

impl Block for SlimeBlock {
    fn id(&self) -> u8 { 165 }
    fn name(&self) -> &'static str { "SlimeBlock" }
}
