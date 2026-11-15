// IronBars 照搬 Nukkit BlockIronBars
use crate::block::Block;

pub struct IronBars;
pub const INSTANCE: IronBars = IronBars;

impl Block for IronBars {
    fn id(&self) -> u8 { 101 }
    fn name(&self) -> &'static str { "IronBars" }
}
