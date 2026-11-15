// ActivatorRail 照搬 Nukkit BlockActivatorRail
use crate::block::Block;

pub struct ActivatorRail;
pub const INSTANCE: ActivatorRail = ActivatorRail;

impl Block for ActivatorRail {
    fn id(&self) -> u8 { 126 }
    fn name(&self) -> &'static str { "ActivatorRail" }
}
