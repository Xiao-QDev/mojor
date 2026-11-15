//! 对应 `rtg/deco/DecoMushrooms.php`。

use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco, offset_pos};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_bush;
use crate::level::generator::world::GenWorld;

pub struct DecoMushrooms {
    pub strength_factor: f64,
    pub max_y: i32,
    pub chance: i32,
    pub loops: i32,
}

impl Default for DecoMushrooms {
    fn default() -> Self {
        Self {
            strength_factor: f(0.0),
            max_y: 255,
            chance: 1,
            loops: 1,
        }
    }
}

impl Deco for DecoMushrooms {
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
        let loop_count = if self.strength_factor > 0.0 {
            self.strength_factor as i32
        } else {
            self.loops
        };
        for _ in 0..loop_count {
            if rand.next_int_bound(self.chance) == 0 {
                let pos = offset_pos(chunk_x, chunk_z).add(
                    rand.next_int_bound(16),
                    rand.next_int_bound(self.max_y),
                    rand.next_int_bound(16),
                );
                let block = if rand.next_boolean() { 39 } else { 40 };
                generate_bush(world, rand, pos, block);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn mycelium_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        // 菌丝体 id=110，蘑菇在菌丝体上不需要低光照
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 110 << 4);
            }
        }
        world
    }

    #[test]
    fn mushrooms_grow_on_mycelium() {
        use crate::level::generator::population::Pos;
        let mut world = mycelium_world();
        let mut rand = JavaRandom::new(333);
        generate_bush(&mut world, &mut rand, Pos::new(8, 64, 8), 39);
        let has = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .any(|(x, z)| (62..72).any(|y| world.block_state(x, y, z) >> 4 == 39));
        assert!(has, "菌丝体上应生成棕蘑菇");
    }
}
