// MonsterSpawner 照搬 Nukkit BlockMonsterSpawner
use crate::block::Block;

pub struct MonsterSpawner;
pub const INSTANCE: MonsterSpawner = MonsterSpawner;

impl Block for MonsterSpawner {
    fn id(&self) -> u8 { 52 }
    fn name(&self) -> &'static str { "MonsterSpawner" }
}
