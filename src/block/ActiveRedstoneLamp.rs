// ActiveRedstoneLamp 照搬 Nukkit BlockActiveRedstoneLamp
use crate::block::Block;

pub struct ActiveRedstoneLamp;
pub const INSTANCE: ActiveRedstoneLamp = ActiveRedstoneLamp;

impl Block for ActiveRedstoneLamp {
    fn id(&self) -> u8 { 124 }
    fn name(&self) -> &'static str { "ActiveRedstoneLamp" }
}
