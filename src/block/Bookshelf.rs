// Bookshelf 照搬 Nukkit BlockBookshelf
use crate::block::Block;

pub struct Bookshelf;
pub const INSTANCE: Bookshelf = Bookshelf;

impl Block for Bookshelf {
    fn id(&self) -> u8 { 47 }
    fn name(&self) -> &'static str { "Bookshelf" }
}
