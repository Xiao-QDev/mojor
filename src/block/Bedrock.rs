// Bedrock 照搬 Nukkit BlockBedrock
use crate::block::Block;

pub struct Bedrock;
pub const INSTANCE: Bedrock = Bedrock;

impl Block for Bedrock {
    fn id(&self) -> u8 { 7 }
    fn name(&self) -> &'static str { "Bedrock" }
}
