//! 对应 `rtg/deco/DecoSponge.php`。
//!
//! 海底海绵团：在 minY..maxY 之间随机取高度，概率通过后调用 `WorldGenSponge`
//! 在水下生成湿海绵团。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, get_ranged_random, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_sponge;
use crate::level::generator::world::GenWorld;

pub struct DecoSponge {
    pub sponge_block: i32,
    pub strength_factor: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub height_type: i32,
    pub chance: i32,
    pub valid_ground_blocks: Vec<i32>,
}

impl Default for DecoSponge {
    fn default() -> Self {
        // 默认湿海绵：19<<4 | 1 = 305。
        Self {
            sponge_block: (19 << 4) | 1,
            strength_factor: f(2.0),
            min_y: 20,
            max_y: 45,
            height_type: 0,
            chance: 10,
            valid_ground_blocks: vec![2, 3, 1, 13, 82, 12],
        }
    }
}

impl Deco for DecoSponge {
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
        let loops = self.strength_factor as i32;
        for _ in 0..loops {
            let column = offset_pos(chunk_x, chunk_z)
                .add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            let y = match self.height_type {
                0 => get_ranged_random(rand, self.min_y, self.max_y),
                _ => surface_height(world, column.x, column.z),
            };
            if y >= self.min_y && y <= self.max_y && rand.next_int_bound(self.chance) == 0 {
                let ground = self.valid_ground_blocks.clone();
                generate_sponge(
                    world,
                    rand,
                    Pos::new(column.x, y, column.z),
                    self.sponge_block,
                    0,
                    &ground,
                );
            }
        }
    }
}
