// StoneWall 照搬 Nukkit BlockStoneWall
use crate::block::Block;

pub struct StoneWall;
pub const INSTANCE: StoneWall = StoneWall;

impl Block for StoneWall {
    fn id(&self) -> u8 { 139 }
    fn name(&self) -> &'static str { "StoneWall" }
}
