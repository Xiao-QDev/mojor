// Coal 照搬 Nukkit BlockCoal
use crate::block::Block;

pub struct Coal;
pub const INSTANCE: Coal = Coal;

impl Block for Coal {
    fn id(&self) -> u8 { 173 }
    fn name(&self) -> &'static str { "Coal" }
}
