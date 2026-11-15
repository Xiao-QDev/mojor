// GlassPane 照搬 Nukkit BlockGlassPane
use crate::block::Block;

pub struct GlassPane;
pub const INSTANCE: GlassPane = GlassPane;

impl Block for GlassPane {
    fn id(&self) -> u8 { 102 }
    fn name(&self) -> &'static str { "GlassPane" }
}
