// Wood 照搬 Nukkit BlockWood
use crate::block::Block;

pub struct Wood;
pub const INSTANCE: Wood = Wood;

impl Block for Wood {
    fn id(&self) -> u8 { 17 }
    fn name(&self) -> &'static str { "Wood" }
}
