// MelonStem 照搬 Nukkit BlockMelonStem
use crate::block::Block;

pub struct MelonStem;
pub const INSTANCE: MelonStem = MelonStem;

impl Block for MelonStem {
    fn id(&self) -> u8 { 105 }
    fn name(&self) -> &'static str { "MelonStem" }
}
