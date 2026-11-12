// 0.14 的方块 id 与 solid/transparent 判定 对齐 Nukkit 的 Block 静态表
pub const AIR: u8 = 0;
pub const STONE: u8 = 1;
pub const GRASS: u8 = 2;
pub const DIRT: u8 = 3;
pub const SAPLING: u8 = 6;
pub const BEDROCK: u8 = 7;
pub const WATER: u8 = 8;
pub const STILL_WATER: u8 = 9;
pub const LAVA: u8 = 10;
pub const STILL_LAVA: u8 = 11;
pub const SAND: u8 = 12;
pub const GRAVEL: u8 = 13;
pub const GOLD_ORE: u8 = 14;
pub const IRON_ORE: u8 = 15;
pub const COAL_ORE: u8 = 16;
pub const LOG: u8 = 17;
pub const LEAVES: u8 = 18;
pub const LAPIS_ORE: u8 = 21;
pub const SANDSTONE: u8 = 24;
pub const TALL_GRASS: u8 = 31;
pub const DEAD_BUSH: u8 = 32;
pub const DANDELION: u8 = 37;
pub const RED_FLOWER: u8 = 38;
pub const DIAMOND_ORE: u8 = 56;
pub const REDSTONE_ORE: u8 = 73;
pub const SNOW_LAYER: u8 = 78;
pub const ICE: u8 = 79;
pub const CACTUS: u8 = 81;
pub const SUGARCANE: u8 = 83;
pub const WATER_LILY: u8 = 111;
pub const LEAVES2: u8 = 161;
pub const LOG2: u8 = 162;
pub const DOUBLE_PLANT: u8 = 175;

// 树干元数据 与 BlockWood 一致
pub const WOOD_OAK: u8 = 0;
pub const WOOD_SPRUCE: u8 = 1;
pub const WOOD_BIRCH: u8 = 2;
pub const WOOD_JUNGLE: u8 = 3;

// 树苗类型 与 BlockSapling 一致
pub const SAPLING_OAK: u8 = 0;
pub const SAPLING_SPRUCE: u8 = 1;
pub const SAPLING_BIRCH: u8 = 2;
pub const SAPLING_JUNGLE: u8 = 3;

// BlockFlowable 与 BlockAir 一类 这些方块不阻挡
pub fn is_solid(id: u8) -> bool {
    !matches!(
        id,
        AIR | SAPLING
            | WATER
            | STILL_WATER
            | LAVA
            | STILL_LAVA
            | TALL_GRASS
            | DEAD_BUSH
            | DANDELION
            | RED_FLOWER
            | SNOW_LAYER
            | SUGARCANE
            | WATER_LILY
            | DOUBLE_PLANT
    )
}

// BlockTransparent 的子类 阳光可以透过它们继续向下
pub fn is_transparent(id: u8) -> bool {
    matches!(
        id,
        AIR | SAPLING
            | WATER
            | STILL_WATER
            | LAVA
            | STILL_LAVA
            | LEAVES
            | TALL_GRASS
            | DEAD_BUSH
            | DANDELION
            | RED_FLOWER
            | SNOW_LAYER
            | ICE
            | CACTUS
            | SUGARCANE
            | WATER_LILY
            | LEAVES2
            | DOUBLE_PLANT
    )
}

// ObjectTree 允许覆盖的方块
pub fn is_overridable(id: u8) -> bool {
    matches!(
        id,
        AIR | SAPLING | LOG | LEAVES | SNOW_LAYER | LOG2 | LEAVES2
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_blocks_light_and_water_does_not() {
        assert!(is_solid(STONE));
        assert!(!is_transparent(STONE));
        assert!(!is_solid(STILL_WATER));
        assert!(is_transparent(STILL_WATER));
        assert!(!is_solid(SNOW_LAYER));
        assert!(is_transparent(SNOW_LAYER));
    }

    #[test]
    fn leaves_block_movement_but_pass_light() {
        assert!(is_solid(LEAVES));
        assert!(is_transparent(LEAVES));
    }
}
