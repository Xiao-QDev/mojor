//! 对应 `rtg/deco/DecoCactus.php`。

use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_cacti;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

pub struct DecoCactus {
    pub loops: i32,
    pub chance: i32,
    pub strength_factor: f64,
    pub max_y: i32,
    pub sand_only: bool,
    pub soil_block: i32,
}

impl Default for DecoCactus {
    fn default() -> Self {
        Self {
            loops: 1,
            chance: 1,
            strength_factor: f(0.0),
            max_y: 255,
            sand_only: false,
            soil_block: 192,
        }
    }
}

impl Deco for DecoCactus {
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
            let column = offset_pos(chunk_x, chunk_z)
                .add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            let y = surface_height(world, column.x, column.z);
            if y <= self.max_y && rand.next_int_bound(self.chance) == 0 {
                generate_cacti(
                    world,
                    rand,
                    column.up_n(y),
                    self.sand_only,
                    0,
                    self.soil_block,
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

    fn sand_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 12 << 4);
                world.set_block_state(x, 62, z, 12 << 4);
            }
        }
        world
    }

    #[test]
    fn cactus_grows_on_sand() {
        let mut world = sand_world();
        let rtg_world = RtgWorld::new(1);
        let mut rand = JavaRandom::new(7);
        let mut info = ChunkInfo::new(0, 0);
        let mut deco = DecoCactus {
            strength_factor: f(32.0),
            max_y: 90,
            chance: 1,
            ..DecoCactus::default()
        };
        deco.generate(&rtg_world, &mut world, &mut rand, 0, 0, 1.0, false, &mut info);

        let cacti = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| (60..90).any(|y| world.block_state(x, y, z) >> 4 == 81))
            .count();
        assert!(cacti > 0, "沙漠地表应生成仙人掌");
    }
}
