// BurningFurnace 照搬 Nukkit BlockBurningFurnace
use crate::block::Block;

pub struct BurningFurnace;
pub const INSTANCE: BurningFurnace = BurningFurnace;

impl Block for BurningFurnace {
    fn id(&self) -> u8 { 62 }
    fn name(&self) -> &'static str { "BurningFurnace" }
}
