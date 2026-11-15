// EnchantingTable 照搬 Nukkit BlockEnchantingTable
use crate::block::Block;

pub struct EnchantingTable;
pub const INSTANCE: EnchantingTable = EnchantingTable;

impl Block for EnchantingTable {
    fn id(&self) -> u8 { 116 }
    fn name(&self) -> &'static str { "EnchantingTable" }
}
