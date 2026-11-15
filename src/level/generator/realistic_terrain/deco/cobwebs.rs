//! 对应 `rtg/deco/DecoCobwebs.php` + `rtg/feature/WorldGenBlock.php`。
//!
//! 蜘蛛网只在空气格上生成，且要求至少有 `min_adjacents` 个相邻方块等于
//! `adjacent_block`（默认空气；黑森林里设为深色橡木原木，让蛛网挂在树干上）。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, get_ranged_random, offset_pos,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

/// 蜘蛛网方块 state：id=30，meta=0。
const WEB: i32 = 30 << 4;
/// 空气 state。
const AIR: i32 = 0;

pub struct DecoCobwebs {
    pub strength_factor: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub chance: i32,
    pub adjacent_block: i32,
    pub min_adjacents: i32,
}

impl Default for DecoCobwebs {
    fn default() -> Self {
        Self {
            strength_factor: f(2.0),
            min_y: 1,
            max_y: 255,
            chance: 10,
            adjacent_block: AIR,
            min_adjacents: 1,
        }
    }
}

impl Deco for DecoCobwebs {
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
            if rand.next_int_bound(self.chance) == 0 {
                let pos = offset_pos(chunk_x, chunk_z).add(
                    rand.next_int_bound(16),
                    get_ranged_random(rand, self.min_y, self.max_y),
                    rand.next_int_bound(16),
                );
                place_cobweb(world, pos, self.adjacent_block, self.min_adjacents);
            }
        }
    }
}

/// 对应 `WorldGenBlock::generate`：目标必须是 `AIR`，且六邻中等于
/// `adjacent_block` 的数量 >= `min_adjacents` 且 > 0，才放置蜘蛛网。
fn place_cobweb(world: &mut GenWorld, pos: Pos, adjacent_block: i32, min_adjacents: i32) {
    if world.block_state(pos.x, pos.y, pos.z) != AIR {
        return;
    }
    let mut count = 0;
    for (dx, dy, dz) in [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ] {
        if world.block_state(pos.x + dx, pos.y + dy, pos.z + dz) == adjacent_block {
            count += 1;
        }
    }
    if count > 0 && count >= min_adjacents {
        world.set_block_state(pos.x, pos.y, pos.z, WEB);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn cobweb_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        // 铺一层深色橡木原木 (woodState(5,false)=2593) 当相邻块
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 2593);
            }
        }
        world
    }

    #[test]
    fn cobweb_places_next_to_dark_oak_log() {
        let mut world = cobweb_world();
        let rtg_world = RtgWorld::new(1);
        let mut rand = JavaRandom::new(7);
        let mut info = ChunkInfo::new(0, 0);
        let mut deco = DecoCobwebs {
            strength_factor: f(64.0),
            min_y: 64,
            max_y: 64,
            chance: 1,
            adjacent_block: 2593,
            min_adjacents: 1,
            ..DecoCobwebs::default()
        };
        deco.generate(&rtg_world, &mut world, &mut rand, 0, 0, 1.0, false, &mut info);

        let webs = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| world.block_state(x, 64, z) == WEB)
            .count();
        assert!(webs > 0, "深色橡木原木旁应生成蜘蛛网");
    }
}
