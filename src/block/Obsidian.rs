// Obsidian 照搬 Nukkit BlockObsidian
use crate::block::Block;

pub struct Obsidian;
pub const INSTANCE: Obsidian = Obsidian;

impl Block for Obsidian {
    fn id(&self) -> u8 { 49 }
    fn name(&self) -> &'static str { "Obsidian" }
}
