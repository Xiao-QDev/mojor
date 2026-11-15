//! 对应 `rtg/deco/DecoDoubleGrass.php`：双格高草丛（稀树草原用）。
//!
//! 每次循环在随机高度取一个坐标，然后围绕它尝试 64 次放置双格高草
//! （方块 175，variant 2 = 高草），能放下就放上下两格。

use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco, offset_pos};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

pub struct DecoDoubleGrass {
    pub strength_factor: f64,
    pub max_y: i32,
    pub loops: i32,
}

impl Default for DecoDoubleGrass {
    /// 对应 `constructDecoDoubleGrass0`。
    fn default() -> Self {
        Self {
            strength_factor: f(0.0),
            max_y: 255,
            loops: 1,
        }
    }
}

impl Deco for DecoDoubleGrass {
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
            let center = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                rand.next_int_bound(self.max_y),
                rand.next_int_bound(16),
            );
            // 对应 `WorldGenDoublePlantRTG(2).generate`：围绕中心随机偏移尝试放高草。
            for _ in 0..64 {
                let pos = center.add(
                    rand.next_int_bound(8) - rand.next_int_bound(8),
                    rand.next_int_bound(4) - rand.next_int_bound(4),
                    rand.next_int_bound(8) - rand.next_int_bound(8),
                );
                if pos.y < 254
                    && world.is_air(pos.x, pos.y, pos.z)
                    && block_rules::can_place_block_at(175, world, pos)
                {
                    block_rules::place_at(175, world, pos, 2);
                }
            }
        }
    }
}
