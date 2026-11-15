//! 对应 `rtg/deco/DecoCrop.php`。
//!
//! 农作物装饰：在草/泥土上生成小麦/胡萝卜/土豆/甜菜根斑块（脚下铺耕地）。
//! 目前 Plains（biome 1/129）用它生成小麦。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, get_ranged_random, offset_pos,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::vanilla::generate_crops;
use crate::level::generator::world::GenWorld;

pub struct DecoCrop {
    pub crop_type: i32,
    pub farm_size: i32,
    pub farm_density: i32,
    pub farm_height: i32,
    pub strength_factor: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub chance: i32,
    pub water: bool,
}

impl Default for DecoCrop {
    fn default() -> Self {
        Self {
            crop_type: 3,       // 小麦
            farm_size: 5,
            farm_density: 50,
            farm_height: 2,
            strength_factor: f(2.0),
            min_y: 63,
            max_y: 255,
            chance: 10,
            water: true,
        }
    }
}

impl Deco for DecoCrop {
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
            let y = get_ranged_random(rand, self.min_y, self.max_y);
            if y >= self.min_y
                && y <= self.max_y
                && rand.next_int_bound(self.chance) == 0
            {
                generate_crops(
                    world,
                    rand,
                    Pos::new(column.x, y, column.z),
                    self.crop_type,
                    self.farm_size,
                    self.farm_density,
                    self.farm_height,
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

    fn grass_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 2 << 4); // 草地
            }
        }
        world
    }

    #[test]
    fn wheat_grows_on_grass() {
        let mut world = grass_world();
        let mut rand = JavaRandom::new(7);
        let ok = generate_crops(&mut world, &mut rand, Pos::new(8, 64, 8), 3, 4, 64, 2, false);
        assert!(ok, "generate_crops 应成功");

        let wheat = (0..16)
            .flat_map(|x| (0..16).map(move |z| (x, z)))
            .filter(|&(x, z)| world.block_state(x, 64, z) >> 4 == 59)
            .count();
        assert!(wheat > 0, "草地上应生成小麦");
    }
}
