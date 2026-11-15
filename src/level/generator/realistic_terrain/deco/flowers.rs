//! 对应 `rtg/deco/DecoFlowersRTG.php` 与 `rtg/feature/WorldGenFlowersRTG.php` 的花部分。
//!
//! 参考实现的花种选择刻意使用 `JavaRandom::fromSystem()`，不消耗区块随机源；
//! 因此同一位置每次新生成时花种可以不同，但坐标和数量仍由世界种子决定。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, get_ranged_random, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

pub struct DecoFlowersRtg {
    pub flowers: Vec<i32>,
    pub strength_factor: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub height_type: i32,
    pub chance: i32,
    pub not_equals_zero_chance: i32,
    pub loops: i32,
}

impl Default for DecoFlowersRtg {
    fn default() -> Self {
        Self {
            flowers: Vec::new(),
            strength_factor: f(0.0),
            min_y: 1,
            max_y: 255,
            height_type: 0,
            chance: 1,
            not_equals_zero_chance: 1,
            loops: 1,
        }
    }
}

impl Deco for DecoFlowersRtg {
    fn generate(
        &mut self,
        _rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        _tree_density_multiplier: f64,
        _has_village: bool,
        _chunk_info: &mut ChunkInfo,
    ) {
        if self.flowers.is_empty() {
            return;
        }
        let loop_count = if self.strength_factor > 0.0 {
            self.strength_factor as i32
        } else {
            self.loops
        };
        for _ in 0..loop_count * 16 {
            let column = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                0,
                rand.next_int_bound(16),
            );
            let y = match self.height_type {
                0 => get_ranged_random(rand, self.min_y, self.max_y),
                1 => surface_height(world, column.x, column.z),
                _ => rand.next_int_bound(self.max_y),
            };
            let generate = if self.not_equals_zero_chance > 1 {
                rand.next_int_bound(self.not_equals_zero_chance) != 0
            } else {
                rand.next_int_bound(self.chance) == 0
            };
            if generate {
                generate_flower(world, column.up_n(y), &self.flowers);
            }
        }
    }
}

fn generate_flower(world: &mut GenWorld, pos: Pos, flowers: &[i32]) -> bool {
    if flowers.is_empty() {
        return false;
    }
    let mut system_rand = JavaRandom::from_system();
    let kind = flowers[system_rand.next_int_bound(flowers.len() as i32) as usize];
    let flower = block_rules::flower_state(kind);
    let block = flower >> 4;
    if world.is_air(pos.x, pos.y, pos.z)
        && pos.y < 255
        && block_rules::can_place_block_at(block, world, pos)
        && block_rules::can_block_stay(block, world, pos)
    {
        world.set_block_state(pos.x, pos.y, pos.z, flower);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn empty_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        GenWorld::new(chunks)
    }

    #[test]
    fn flower_generates_on_grass() {
        let mut world = empty_world();
        world.set_block_state(8, 63, 8, 32);
        assert!(generate_flower(
            &mut world,
            Pos::new(8, 64, 8),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
        ));
        assert!(matches!(world.block_state(8, 64, 8) >> 4, 37 | 38));
    }

    #[test]
    fn flower_kind_does_not_consume_chunk_random() {
        let rtg_world = RtgWorld::new(1);
        let mut world = empty_world();
        let mut rand = JavaRandom::new(99);
        let mut expected = rand.clone();
        let mut info = ChunkInfo::new(0, 0);
        let mut deco = DecoFlowersRtg {
            flowers: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
            strength_factor: f(6.0),
            max_y: 128,
            ..DecoFlowersRtg::default()
        };
        deco.generate(
            &rtg_world, &mut world, &mut rand, 0, 0, 1.0, false, &mut info,
        );

        for _ in 0..96 {
            expected.next_int_bound(16);
            expected.next_int_bound(16);
            expected.next_int_bound(128);
            expected.next_int_bound(1);
        }
        assert_eq!(rand.state(), expected.state());
    }
}
