// 方块的光照属性表 对齐 vanilla BlockState 的三个光照口径
// dampening  对应 getLightDampening：0 表示光无衰减穿过，也决定源列隔断
//            （ChunkSkyLightSources.isEdgeOccluded：dampening != 0 即隔断该列）
// opacity    对应 getOpacity = max(1, dampening)：BFS 每走一格扣掉的光
//            完全不透光的方块给 16，保证 from - opacity 恒 <= 0 光进不去
// emission   对应 getLightEmission：火把一类的自身发光
// 0.14 没有 faceShapeOccludes（方块形状系统） 传播遮挡完全由 opacity 表达
use crate::level::generator::blocks::{
    AIR, CACTUS, DANDELION, DEAD_BUSH, DOUBLE_PLANT, ICE, LEAVES, LEAVES2, LAVA, RED_FLOWER,
    SAPLING, SNOW_LAYER, STILL_LAVA, STILL_WATER, SUGARCANE, TALL_GRASS, WATER, WATER_LILY,
};

pub fn dampening(id: u8) -> u8 {
    match id {
        // 空气与植物 雪层 甘蔗 睡莲 仙人掌
        AIR | SAPLING | TALL_GRASS | DEAD_BUSH | DANDELION | RED_FLOWER | SNOW_LAYER
        | SUGARCANE | WATER_LILY | DOUBLE_PLANT | CACTUS => 0,
        // 玻璃 与 0.14 玩家可放置的透光件
        20 |          // GLASS
        50 |          // TORCH
        55 |          // REDSTONE_WIRE
        63 | 68 |     // SIGN 与墙式告示牌
        64 | 71 |     // 木门与铁门（0.14 无形状系统 门按透光处理）
        65 |          // LADDER
        66 | 27 | 28| // 三种铁轨
        69 |          // LEVER
        70 | 72 |     // 石质与木质压力板
        77 |          // STONE_BUTTON
        96 |          // TRAPDOOR
        92 |          // CAKE
        107 | 183 | 184 | 185 | 186 | 187 => 0, // 栅栏门各木种
        // 水 树叶 蜘蛛网 挡一格光 同时截断天空源列
        WATER | STILL_WATER | LEAVES | LEAVES2 | 30 => 1, // 30 = COBWEB
        // 冰挡三格光
        ICE => 3,
        // 其余全部完全不透光
        _ => 15,
    }
}

// BFS 的传播衰减 vanilla getOpacity = max(1, dampening)
// dampening=15 的方块给 16：光减到 0 以下 自然进不去也不穿出
pub fn opacity(id: u8) -> u8 {
    match dampening(id) {
        0 => 1,
        d if d >= 15 => 16,
        d => d,
    }
}

// 自身发光表 对齐 Nukkit 0.14 的 getLightLevel
pub fn emission(id: u8) -> u8 {
    match id {
        50 => 14,             // TORCH
        51 => 15,             // FIRE
        LAVA | STILL_LAVA => 15,
        89 => 15,             // GLOWSTONE
        91 => 15,             // JACK_O_LANTERN
        62 => 13,             // BURNING_FURNACE
        75 => 7,              // REDSTONE_TORCH(亮)
        124 => 15,            // REDSTONE_LAMP(亮)
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::blocks::{DIRT, GRASS, STONE};

    #[test]
    fn stone_blocks_light_and_air_does_not() {
        assert_eq!(opacity(STONE), 16);
        assert_eq!(dampening(STONE), 15);
        assert_eq!(opacity(AIR), 1);
        assert_eq!(dampening(AIR), 0);
        assert_eq!(dampening(GRASS), 15);
        assert_eq!(dampening(DIRT), 15);
    }

    #[test]
    fn water_and_leaves_dampen_but_cut_the_source_column() {
        assert_eq!(opacity(WATER), 1);
        assert_eq!(dampening(WATER), 1);
        assert_eq!(opacity(LEAVES), 1);
        assert_eq!(dampening(LEAVES), 1);
        assert_eq!(opacity(ICE), 3);
    }

    #[test]
    fn torch_and_lava_emit() {
        assert_eq!(emission(50), 14);
        assert_eq!(emission(LAVA), 15);
        assert_eq!(emission(89), 15);
        assert_eq!(emission(STONE), 0);
    }

    #[test]
    fn glass_keeps_the_column_open() {
        assert_eq!(dampening(20), 0);
        assert_eq!(opacity(20), 1);
    }
}
