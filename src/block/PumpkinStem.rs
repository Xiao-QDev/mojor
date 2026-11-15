// PumpkinStem 照搬 Nukkit BlockPumpkinStem
use crate::block::Block;

pub struct PumpkinStem;
pub const INSTANCE: PumpkinStem = PumpkinStem;

impl Block for PumpkinStem {
    fn id(&self) -> u8 { 104 }
    fn name(&self) -> &'static str { "PumpkinStem" }
}
