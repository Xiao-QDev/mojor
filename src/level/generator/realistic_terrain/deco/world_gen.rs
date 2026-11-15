//! 对应 `rtg/deco/DecoWorldGen.php`。
//!
//! 把任意 `WorldGen`（目前是 `WorldGenBigMushroom`）包成 `Deco`：在区块内随机
//! 选列、取地表高度、过 `min_y/max_y/chance` 后调用 `worldgen.generate`。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, offset_pos, surface_height,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::WorldGen;
use crate::level::generator::world::GenWorld;

pub struct DecoWorldGen {
    pub worldgen: Box<dyn WorldGen>,
    pub loops: i32,
    pub chance: i32,
    pub min_y: i32,
    pub max_y: i32,
}

impl DecoWorldGen {
    pub fn new(worldgen: Box<dyn WorldGen>) -> Self {
        Self {
            worldgen,
            loops: 1,
            chance: 1,
            min_y: 1,
            max_y: 255,
        }
    }
}

impl Deco for DecoWorldGen {
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
        // PHP: `for($i = 0; $i <= $loops; $i++)`，共 loops+1 次。
        for _ in 0..=self.loops {
            let pos = offset_pos(chunk_x, chunk_z).add(
                rand.next_int_bound(16),
                0,
                rand.next_int_bound(16),
            );
            let y = surface_height(world, pos.x, pos.z);
            if y >= self.min_y && y <= self.max_y && rand.next_int_bound(self.chance) == 0 {
                self.worldgen.generate(world, rand, Pos::new(pos.x, y, pos.z));
            }
        }
    }
}
