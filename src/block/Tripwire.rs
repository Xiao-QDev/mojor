// Tripwire 照搬 Nukkit BlockTripwire
use crate::block::Block;

pub struct Tripwire;
pub const INSTANCE: Tripwire = Tripwire;

impl Block for Tripwire {
    fn id(&self) -> u8 { 132 }
    fn name(&self) -> &'static str { "Tripwire" }
}
