//! 原版方块 / 材质属性（表由 `tools/mcpe_emu/blockprops.py` 从 libminecraftpe.so 导出）。

pub use super::blockprops_table::{VINE_EAST, VINE_NORTH, VINE_SOUTH, VINE_WEST};
use super::blockprops_table::BLOCK_PROPS;

/// MaterialType
pub mod mat {
    pub const AIR: u8 = 0;
    pub const WATER: u8 = 5;
    pub const LAVA: u8 = 6;
    pub const LEAVES: u8 = 7;
}

/// BlockProperty
pub const PROP_LEAVES: u32 = 0x20;
pub const PROP_SOLID_BLOCKING: u32 = 0x100000;
pub const PROP_TOP_SOLID_BLOCKING: u32 = 0x80000;
pub const PROP_STAIR: u32 = 0x1;
pub const PROP_HALF_SLAB: u32 = 0x2;

#[inline]
fn row(id: u8) -> &'static (u8, u8, u8, bool, bool, u32, i32, bool) {
    &BLOCK_PROPS[id as usize]
}

/// `Block::getMaterial().type`
#[inline]
pub fn material(id: u8) -> u8 {
    row(id).0
}
/// `Material::getBlocksMotion`
#[inline]
pub fn blocks_motion(id: u8) -> bool {
    row(id).1 & 1 != 0
}
/// `Material::isSolid`
#[inline]
pub fn mat_solid(id: u8) -> bool {
    row(id).1 & 2 != 0
}
/// `Material::isLiquid`
#[inline]
pub fn is_liquid(id: u8) -> bool {
    row(id).1 & 4 != 0
}
/// `Material::isReplaceable`
#[inline]
pub fn replaceable(id: u8) -> bool {
    row(id).1 & 8 != 0
}
/// `Material::isFlammable`
#[inline]
pub fn flammable(id: u8) -> bool {
    row(id).1 & 16 != 0
}
/// `Material::isSolidBlocking`
#[inline]
pub fn mat_solid_blocking(id: u8) -> bool {
    row(id).1 & 32 != 0
}
/// `Block::mLightBlock`
#[inline]
pub fn light_block(id: u8) -> i32 {
    row(id).2 as i32
}
/// `Block::mSolid`
#[inline]
pub fn msolid(id: u8) -> bool {
    row(id).3
}
/// `Block::isSolid()`
#[inline]
pub fn block_is_solid(id: u8) -> bool {
    row(id).4
}
#[inline]
pub fn props(id: u8) -> u32 {
    row(id).5
}
#[inline]
pub fn shape(id: u8) -> i32 {
    row(id).6
}
/// `Block::isWaterBlocking`
#[inline]
pub fn water_blocking(id: u8) -> bool {
    row(id).7
}

/// `BlockSource::isSolidBlockingBlock`
pub fn solid_blocking_block(id: u8) -> bool {
    mat_solid_blocking(id) && (shape(id) == 0 || props(id) & PROP_SOLID_BLOCKING != 0)
}

/// `BlockSource::isTopSolidBlocking(Block*, data)`
pub fn top_solid_blocking(id: u8, data: u8) -> bool {
    let p = props(id);
    if p & PROP_TOP_SOLID_BLOCKING != 0 {
        return true;
    }
    if mat_solid_blocking(id) && block_is_solid(id) {
        return true;
    }
    if p & PROP_STAIR != 0 {
        return (data >> 2) & 1 != 0;
    }
    if p & PROP_HALF_SLAB != 0 {
        return (data >> 3) & 1 != 0;
    }
    // TopSnowBlock：满 8 层
    id == 78 && (data & 7) + 1 == 8
}
