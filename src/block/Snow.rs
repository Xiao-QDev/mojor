// Snow 照搬 Nukkit BlockSnow
use crate::block::Block;

pub struct Snow;
pub const INSTANCE: Snow = Snow;

impl Block for Snow {
    fn id(&self) -> u8 { 80 }
    fn name(&self) -> &'static str { "Snow" }
}
