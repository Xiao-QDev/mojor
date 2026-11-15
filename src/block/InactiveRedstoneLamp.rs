// InactiveRedstoneLamp 照搬 Nukkit BlockInactiveRedstoneLamp
use crate::block::Block;

pub struct InactiveRedstoneLamp;
pub const INSTANCE: InactiveRedstoneLamp = InactiveRedstoneLamp;

impl Block for InactiveRedstoneLamp {
    fn id(&self) -> u8 { 123 }
    fn name(&self) -> &'static str { "InactiveRedstoneLamp" }
}
