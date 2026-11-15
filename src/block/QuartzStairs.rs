// QuartzStairs 照搬 Nukkit BlockQuartzStairs
use crate::block::Block;

pub struct QuartzStairs;
pub const INSTANCE: QuartzStairs = QuartzStairs;

impl Block for QuartzStairs {
    fn id(&self) -> u8 { 156 }
    fn name(&self) -> &'static str { "QuartzStairs" }
}
