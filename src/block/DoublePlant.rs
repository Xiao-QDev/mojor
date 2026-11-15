// DoublePlant 照搬 Nukkit BlockDoublePlant
use crate::block::Block;

pub struct DoublePlant;
pub const INSTANCE: DoublePlant = DoublePlant;

impl Block for DoublePlant {
    fn id(&self) -> u8 { 175 }
    fn name(&self) -> &'static str { "DoublePlant" }
}
