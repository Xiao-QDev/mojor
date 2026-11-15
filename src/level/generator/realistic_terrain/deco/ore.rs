//! 对应 `rtg/deco/OreSettings.php` + `rtg/deco/OreDecoration.php`。
//!
//! 地下矿石生成：在石头变种中散布泥土、沙砾、花岗岩、闪长岩、安山岩、煤、铁、
//! 金、红石、钻石、青金石。biome 无关，所有区块都跑同一套矿石配置。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_minable;
use crate::level::generator::world::GenWorld;

/// 对应 `OreSettings`：每种矿石的团大小、数量、高度范围。
#[derive(Clone, Debug)]
pub struct OreSettings {
    pub dirt_size: i32,
    pub dirt_count: i32,
    pub dirt_min: i32,
    pub dirt_max: i32,
    pub gravel_size: i32,
    pub gravel_count: i32,
    pub gravel_min: i32,
    pub gravel_max: i32,
    pub granite_size: i32,
    pub granite_count: i32,
    pub granite_min: i32,
    pub granite_max: i32,
    pub diorite_size: i32,
    pub diorite_count: i32,
    pub diorite_min: i32,
    pub diorite_max: i32,
    pub andesite_size: i32,
    pub andesite_count: i32,
    pub andesite_min: i32,
    pub andesite_max: i32,
    pub coal_size: i32,
    pub coal_count: i32,
    pub coal_min: i32,
    pub coal_max: i32,
    pub iron_size: i32,
    pub iron_count: i32,
    pub iron_min: i32,
    pub iron_max: i32,
    pub gold_size: i32,
    pub gold_count: i32,
    pub gold_min: i32,
    pub gold_max: i32,
    pub redstone_size: i32,
    pub redstone_count: i32,
    pub redstone_min: i32,
    pub redstone_max: i32,
    pub diamond_size: i32,
    pub diamond_count: i32,
    pub diamond_min: i32,
    pub diamond_max: i32,
    pub lapis_size: i32,
    pub lapis_count: i32,
    pub lapis_center: i32,
    pub lapis_spread: i32,
}

impl Default for OreSettings {
    fn default() -> Self {
        Self {
            dirt_size: 33, dirt_count: 10, dirt_min: 0, dirt_max: 256,
            gravel_size: 33, gravel_count: 8, gravel_min: 0, gravel_max: 256,
            granite_size: 33, granite_count: 10, granite_min: 0, granite_max: 80,
            diorite_size: 33, diorite_count: 10, diorite_min: 0, diorite_max: 80,
            andesite_size: 33, andesite_count: 10, andesite_min: 0, andesite_max: 80,
            coal_size: 17, coal_count: 20, coal_min: 0, coal_max: 128,
            iron_size: 9, iron_count: 20, iron_min: 0, iron_max: 64,
            gold_size: 9, gold_count: 2, gold_min: 0, gold_max: 32,
            redstone_size: 8, redstone_count: 8, redstone_min: 0, redstone_max: 16,
            diamond_size: 8, diamond_count: 1, diamond_min: 0, diamond_max: 16,
            lapis_size: 7, lapis_count: 1, lapis_center: 16, lapis_spread: 16,
        }
    }
}

/// 矿石方块 state。
pub const DIRT_ORE: i32 = 48;      // block 3
pub const GRAVEL_ORE: i32 = 208;   // block 13
pub const DIORITE_ORE: i32 = 19;   // block 1 meta 3
pub const GRANITE_ORE: i32 = 17;   // block 1 meta 1
pub const ANDESITE_ORE: i32 = 21;  // block 1 meta 5
pub const COAL_ORE: i32 = 256;     // block 16
pub const IRON_ORE: i32 = 240;     // block 15
pub const GOLD_ORE: i32 = 224;     // block 14
pub const REDSTONE_ORE: i32 = 1168;// block 73
pub const DIAMOND_ORE: i32 = 896;  // block 56
pub const LAPIS_ORE: i32 = 336;    // block 21

/// 对应 `OreDecoration`：按配置在区块内散布所有矿石。
pub struct OreDecoration {
    pub settings: OreSettings,
}

impl Default for OreDecoration {
    fn default() -> Self {
        Self { settings: OreSettings::default() }
    }
}

impl OreDecoration {
    pub fn generate(&self, world: &mut GenWorld, rand: &mut JavaRandom, chunk_x: i32, chunk_z: i32) {
        let ox = chunk_x * 16;
        let oz = chunk_z * 16;
        let s = &self.settings;

        self.gen_standard_ore1(world, rand, ox, oz, s.dirt_count, DIRT_ORE, s.dirt_size, s.dirt_min, s.dirt_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.gravel_count, GRAVEL_ORE, s.gravel_size, s.gravel_min, s.gravel_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.diorite_count, DIORITE_ORE, s.diorite_size, s.diorite_min, s.diorite_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.granite_count, GRANITE_ORE, s.granite_size, s.granite_min, s.granite_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.andesite_count, ANDESITE_ORE, s.andesite_size, s.andesite_min, s.andesite_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.coal_count, COAL_ORE, s.coal_size, s.coal_min, s.coal_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.iron_count, IRON_ORE, s.iron_size, s.iron_min, s.iron_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.gold_count, GOLD_ORE, s.gold_size, s.gold_min, s.gold_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.redstone_count, REDSTONE_ORE, s.redstone_size, s.redstone_min, s.redstone_max);
        self.gen_standard_ore1(world, rand, ox, oz, s.diamond_count, DIAMOND_ORE, s.diamond_size, s.diamond_min, s.diamond_max);
        self.gen_standard_ore2(world, rand, ox, oz, s.lapis_count, LAPIS_ORE, s.lapis_size, s.lapis_center, s.lapis_spread);
    }

    /// 对应 `genStandardOre1`：在 min..max 间随机取高度。
    fn gen_standard_ore1(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        ox: i32,
        oz: i32,
        count: i32,
        ore_block: i32,
        size: i32,
        min_height: i32,
        max_height: i32,
    ) {
        let (min, max) = if max_height < min_height {
            (max_height, min_height)
        } else if max_height == min_height {
            if min_height < 255 {
                (min_height, min_height + 1)
            } else {
                (min_height - 1, min_height)
            }
        } else {
            (min_height, max_height)
        };
        for _ in 0..count {
            let pos = Pos::new(
                ox + rand.next_int_bound(16),
                rand.next_int_bound(max - min) + min,
                oz + rand.next_int_bound(16),
            );
            generate_minable(world, rand, pos, ore_block, size);
        }
    }

    /// 对应 `genStandardOre2`：青金石用中心±spread 的正态-ish 分布。
    fn gen_standard_ore2(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        ox: i32,
        oz: i32,
        count: i32,
        ore_block: i32,
        size: i32,
        center: i32,
        spread: i32,
    ) {
        for _ in 0..count {
            let y = rand.next_int_bound(spread) + rand.next_int_bound(spread) + center - spread;
            let pos = Pos::new(ox + rand.next_int_bound(16), y, oz + rand.next_int_bound(16));
            generate_minable(world, rand, pos, ore_block, size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn stone_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        // y 0..64 全是石头 (state 16)
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..64 {
                    world.set_block_state(x, y, z, 16);
                }
            }
        }
        world
    }

    #[test]
    fn coal_ore_generates_in_stone() {
        let mut world = stone_world();
        let mut rand = JavaRandom::new(42);
        let ore = OreDecoration::default();
        ore.generate(&mut world, &mut rand, 0, 0);

        let coal = (0..16)
            .flat_map(|x| (0..64).flat_map(move |y| (0..16).map(move |z| (x, y, z))))
            .filter(|&(x, y, z)| world.block_state(x, y, z) == COAL_ORE)
            .count();
        assert!(coal > 0, "石头世界里应生成煤矿");
    }

    #[test]
    fn diamond_only_near_bedrock() {
        let mut world = stone_world();
        let mut rand = JavaRandom::new(123);
        let ore = OreDecoration::default();
        ore.generate(&mut world, &mut rand, 0, 0);

        let diamond_high = (0..16)
            .flat_map(|x| (32..64).flat_map(move |y| (0..16).map(move |z| (x, y, z))))
            .filter(|&(x, y, z)| world.block_state(x, y, z) == DIAMOND_ORE)
            .count();
        // 钻石 maxY=16，y>=32 不应有钻石
        assert_eq!(diamond_high, 0, "钻石不应出现在 y>=32");
    }
}
