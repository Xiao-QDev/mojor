//! 对应 `rtg/deco/DecoShrub.php`；形状复用 `WorldGenShrubRTG` 的已移植实现。

use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, offset_pos, surface_height, tweak_tree_leaves,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_shrub_with_sand;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

pub struct DecoShrub {
    pub size: i32,
    pub use_default_random: bool,
    pub sand: bool,
    pub random_log_blocks: Vec<i32>,
    pub random_leaves_blocks: Vec<i32>,
    pub loop_multiplier: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub chance: i32,
    pub not_equals_zero_chance: i32,
    pub loops: i32,
    pub min_size: i32,
    pub max_size: i32,
    pub log_block: i32,
    pub leaves_block: i32,
}

impl Default for DecoShrub {
    fn default() -> Self {
        Self {
            size: -1,
            use_default_random: false,
            sand: false,
            random_log_blocks: vec![272, block_rules::wood_state(1, false)],
            random_leaves_blocks: vec![296, block_rules::wood_state(1, true)],
            loop_multiplier: f(1.0),
            min_y: 1,
            max_y: 255,
            chance: 1,
            not_equals_zero_chance: 1,
            loops: 1,
            min_size: 3,
            max_size: 4,
            log_block: 272,
            leaves_block: 296,
        }
    }
}

impl Deco for DecoShrub {
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
        if has_village {
            return;
        }
        if self.size == -1 {
            self.size = rand.next_int_bound(4) + 1;
        }
        if self.min_size > 0 && self.max_size > 0 && self.max_size >= self.min_size {
            self.size = self.min_size + rand.next_int_bound(self.max_size - self.min_size + 1);
        }
        if self.use_default_random
            && !self.random_log_blocks.is_empty()
            && self.random_log_blocks.len() == self.random_leaves_blocks.len()
        {
            let index = rand.next_int_bound(self.random_log_blocks.len() as i32) as usize;
            self.log_block = self.random_log_blocks[index];
            self.leaves_block = self.random_leaves_blocks[index];
        }
        self.leaves_block = tweak_tree_leaves(self.leaves_block, false, true);

        let loop_count = f(f(self.loops as f64) * self.loop_multiplier) as i32;
        for _ in 0..loop_count {
            let column = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                0,
                rand.next_int_bound(16),
            );
            let pos = column.up_n(surface_height(world, column.x, column.z));
            if pos.y < self.min_y || pos.y > self.max_y {
                continue;
            }
            let generate = if self.not_equals_zero_chance > 1 {
                rand.next_int_bound(self.not_equals_zero_chance) != 0
            } else {
                rand.next_int_bound(self.chance) == 0
            };
            if generate {
                let materials = TreeMaterials {
                    log: self.log_block,
                    leaves: self.leaves_block,
                    ..TreeMaterials::OAK
                };
                generate_shrub_with_sand(world, rand, pos, self.size, materials, self.sand);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    #[test]
    fn shrub_deco_generates_configured_loops() {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 32);
                world.set_block_state(x, 62, z, 48);
            }
        }
        let rtg_world = RtgWorld::new(1);
        let mut rand = JavaRandom::new(8);
        let mut info = ChunkInfo::new(0, 0);
        let mut deco = DecoShrub {
            loop_multiplier: f(2.0),
            ..DecoShrub::default()
        };
        deco.generate(
            &rtg_world, &mut world, &mut rand, 0, 0, 1.0, false, &mut info,
        );

        assert!((3..=4).contains(&deco.size));
        let logs = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| world.block_state(x, 63, z) >> 4 == 17)
            .count();
        assert!(logs > 0, "独立灌木应在地表生成原木");
    }
}
