// LitPumpkin 照搬 Nukkit BlockLitPumpkin
use crate::block::Block;

pub struct LitPumpkin;
pub const INSTANCE: LitPumpkin = LitPumpkin;

impl Block for LitPumpkin {
    fn id(&self) -> u8 { 91 }
    fn name(&self) -> &'static str { "LitPumpkin" }
}
