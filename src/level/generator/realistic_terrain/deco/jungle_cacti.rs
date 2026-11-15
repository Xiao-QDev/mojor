//! 对应 `rtg/deco/DecoJungleCacti.php` + `rtg/feature/WorldGenJungleCacti.php`：
//! 丛林仙人掌（在沙/草/土上随机生长的仙人掌柱）。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{ChunkInfo, Deco, offset_pos};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

pub struct DecoJungleCacti {
    pub strength_factor: f64,
    pub max_y: i32,
    pub sand_only: bool,
    pub extra_height: i32,
}

impl Default for DecoJungleCacti {
    /// 对应 `constructDecoJungleCacti0`。
    fn default() -> Self {
        Self {
            strength_factor: f(8.0),
            max_y: 255,
            sand_only: false,
            extra_height: 7,
        }
    }
}

impl Deco for DecoJungleCacti {
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
        let loop_count = self.strength_factor as i32;
        for _ in 0..loop_count {
            let pos = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                rand.next_int_bound(self.max_y),
                rand.next_int_bound(16),
            );
            let extra = rand.next_int_bound(self.extra_height);
            generate_jungle_cacti(world, rand, pos, self.sand_only, extra);
        }
    }
}

/// 对应 `WorldGenJungleCacti::generate`。
fn generate_jungle_cacti(
    world: &mut GenWorld,
    rand: &mut JavaRandom,
    pos: Pos,
    sand_only: bool,
    extra_height: i32,
) {
    let (x, y, z) = (pos.x, pos.y, pos.z);
    for _ in 0..10 {
        let i1 = x + rand.next_int_bound(8) - rand.next_int_bound(8);
        let j1 = y + rand.next_int_bound(4) - rand.next_int_bound(4);
        let k1 = z + rand.next_int_bound(8) - rand.next_int_bound(8);
        if world.is_air(i1, j1, k1) {
            let below = world.block_state(i1, j1 - 1, k1);
            // 192 = 沙(12<<4), 32 = 草(2<<4), 48 = 泥(3<<4)
            let on_sand = below == 192;
            let on_soil = !sand_only && (below == 32 || below == 48);
            if on_sand || on_soil {
                let inner = rand.next_int_bound(3);
                let l1 = 1 + rand.next_int_bound(inner + 1);
                if below == 32 || below == 48 {
                    world.set_block_state(i1, j1 - 1, k1, 193);
                }
                for i2 in 0..(l1 + extra_height) {
                    let p = Pos::new(i1, j1 + i2, k1);
                    if block_rules::can_block_stay(81, world, p) {
                        world.set_block_state(i1, j1 + i2, k1, 81 << 4);
                    }
                }
            }
        }
    }
}
