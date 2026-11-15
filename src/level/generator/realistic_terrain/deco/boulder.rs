//! 对应 `rtg/deco/DecoBoulder.php`。

use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, get_ranged_random, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_blob;
use crate::level::generator::world::GenWorld;

pub struct DecoBoulder {
    pub boulder_block: i32,
    pub strength_factor: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub height_type: i32,
    pub chance: i32,
    pub water: bool,
    pub valid_ground_blocks: Vec<i32>,
}

impl Default for DecoBoulder {
    fn default() -> Self {
        Self {
            boulder_block: 64,
            strength_factor: f(2.0),
            min_y: 60,
            max_y: 255,
            height_type: 1,
            chance: 10,
            water: true,
            valid_ground_blocks: vec![2, 3, 1, 13, 82, 12],
        }
    }
}

impl Deco for DecoBoulder {
    fn generate(
        &mut self,
        _rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        _tree_density_multiplier: f64,
        has_village: bool,
        _chunk_info: &mut ChunkInfo,
    ) {
        let loops = self.strength_factor as i32;
        for _ in 0..loops {
            let column = offset_pos(chunk_x, chunk_z)
                .add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            let y = match self.height_type {
                0 => get_ranged_random(rand, self.min_y, self.max_y),
                _ => surface_height(world, column.x, column.z),
            };
            if y < self.min_y || y > self.max_y || rand.next_int_bound(self.chance) != 0 {
                continue;
            }
            let target = column.up_n(y);
            if !has_village || block_rules::area_is_air(world, target, 2) {
                generate_blob(
                    world,
                    rand,
                    target,
                    self.boulder_block,
                    0,
                    &self.valid_ground_blocks,
                    self.water,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn stone_platform_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                // 石头 id=1 在 validGroundBlocks 里
                world.set_block_state(x, 63, z, 1 << 4);
                world.set_block_state(x, 62, z, 1 << 4);
            }
        }
        world
    }

    #[test]
    fn boulder_places_cobblestone_on_stone() {
        use crate::level::generator::population::Pos;
        let mut world = stone_platform_world();
        let mut rand = JavaRandom::new(99);
        let placed = generate_blob(
            &mut world,
            &mut rand,
            Pos::new(8, 64, 8),
            64,
            0,
            &[2, 3, 1, 13, 82, 12],
            true,
        );
        assert!(placed, "石头地表应能放置巨石");
        let cobbles = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| (60..70).any(|y| world.block_state(x, y, z) == 64))
            .count();
        assert!(cobbles > 0, "应生成鹅卵石巨石");
    }
}
