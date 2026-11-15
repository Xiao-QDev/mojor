// WaterLily 照搬 Nukkit BlockWaterLily
use crate::block::Block;

pub struct WaterLily;
pub const INSTANCE: WaterLily = WaterLily;

impl Block for WaterLily {
    fn id(&self) -> u8 { 111 }
    fn name(&self) -> &'static str { "WaterLily" }
}
