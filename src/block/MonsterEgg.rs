// MonsterEgg 照搬 Nukkit BlockMonsterEgg
use crate::block::Block;

pub struct MonsterEgg;
pub const INSTANCE: MonsterEgg = MonsterEgg;

impl Block for MonsterEgg {
    fn id(&self) -> u8 { 97 }
    fn name(&self) -> &'static str { "MonsterEgg" }
}
