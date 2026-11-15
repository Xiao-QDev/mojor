//! 对应 `rtg/deco/DecoReed.php`。

use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco, offset_pos};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_reed;
use crate::level::generator::world::GenWorld;

pub struct DecoReed {
    pub strength_factor: f64,
    pub max_y: i32,
    pub loops: i32,
}

impl Default for DecoReed {
    fn default() -> Self {
        Self {
            strength_factor: f(0.0),
            max_y: 255,
            loops: 1,
        }
    }
}

impl Deco for DecoReed {
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
            let pos = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                rand.next_int_bound(self.max_y),
                rand.next_int_bound(16),
            );
            generate_reed(world, rand, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    /// 中间一格沙子，四邻是水，甘蔗应能长出来。
    fn sand_island_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        // y=63 沙子小岛，四邻水
        world.set_block_state(8, 63, 8, 12 << 4);
        for (dx, dz) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            world.set_block_state(8 + dx, 63, 8 + dz, 9 << 4);
        }
        world
    }

    #[test]
    fn reed_grows_next_to_water() {
        use crate::level::generator::population::Pos;
        use crate::level::generator::realistic_terrain::tree::vanilla::generate_reed;
        let mut world = sand_island_world();
        let mut rand = JavaRandom::new(42);
        // 甘蔗应长在沙子(8,63,8)上方，即 y=64
        generate_reed(&mut world, &mut rand, Pos::new(8, 64, 8));
        let reeds = (60..72).filter(|&y| world.block_state(8, y, 8) >> 4 == 83).count();
        assert!(reeds > 0, "水边应生成甘蔗");
    }
}
