// Anvil 照搬 Nukkit BlockAnvil
use crate::block::Block;

pub struct Anvil;
pub const INSTANCE: Anvil = Anvil;

impl Block for Anvil {
    fn id(&self) -> u8 { 145 }
    fn name(&self) -> &'static str { "Anvil" }
}
