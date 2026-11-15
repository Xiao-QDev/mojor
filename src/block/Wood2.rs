// Wood2 照搬 Nukkit BlockWood2
use crate::block::Block;

pub struct Wood2;
pub const INSTANCE: Wood2 = Wood2;

impl Block for Wood2 {
    fn id(&self) -> u8 { 162 }
    fn name(&self) -> &'static str { "Wood2" }
}
