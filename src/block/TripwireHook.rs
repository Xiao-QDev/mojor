// TripwireHook 照搬 Nukkit BlockTripwireHook
use crate::block::Block;

pub struct TripwireHook;
pub const INSTANCE: TripwireHook = TripwireHook;

impl Block for TripwireHook {
    fn id(&self) -> u8 { 131 }
    fn name(&self) -> &'static str { "TripwireHook" }
}
