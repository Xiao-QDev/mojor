// Workbench 照搬 Nukkit BlockWorkbench
use crate::block::Block;

pub struct Workbench;
pub const INSTANCE: Workbench = Workbench;

impl Block for Workbench {
    fn id(&self) -> u8 { 58 }
    fn name(&self) -> &'static str { "Workbench" }
}
