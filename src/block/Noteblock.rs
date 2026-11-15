// Noteblock 照搬 Nukkit BlockNoteblock
use crate::block::Block;

pub struct Noteblock;
pub const INSTANCE: Noteblock = Noteblock;

impl Block for Noteblock {
    fn id(&self) -> u8 { 25 }
    fn name(&self) -> &'static str { "Noteblock" }
}
