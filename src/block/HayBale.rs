// HayBale 照搬 Nukkit BlockHayBale
use crate::block::Block;

pub struct HayBale;
pub const INSTANCE: HayBale = HayBale;

impl Block for HayBale {
    fn id(&self) -> u8 { 170 }
    fn name(&self) -> &'static str { "HayBale" }
}
