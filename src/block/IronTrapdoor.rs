// IronTrapdoor 照搬 Nukkit BlockIronTrapdoor
use crate::block::Block;

pub struct IronTrapdoor;
pub const INSTANCE: IronTrapdoor = IronTrapdoor;

impl Block for IronTrapdoor {
    fn id(&self) -> u8 { 167 }
    fn name(&self) -> &'static str { "IronTrapdoor" }
}
