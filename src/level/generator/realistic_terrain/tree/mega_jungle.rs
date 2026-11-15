//! 对应 `rtg/feature/WorldGenMegaJungle.php` + `WorldGenHugeTrees.php`：
//! 巨型丛林树（2x2 树干 + 树冠 + 侧枝 + 藤蔓）。
//!
//! 这是 Minecraft 原版算法，不是 RTG 自定义树。作为 `Tree` trait 的实现者
//! 接入 `DecoTree`（tree_type=1 路径），用丛林木材（variant 3）。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

/// 巨型丛林树。`base` 只用来存 log/leaves 方块，不参与 RTG 树的尺寸逻辑。
#[derive(Clone, Debug)]
pub struct TreeMegaJungle {
    pub base: TreeRTG,
    pub base_height: i32,
    pub extra_random_height: i32,
}

impl Default for TreeMegaJungle {
    /// 对应 `new WorldGenMegaJungle(false, 10, 27, woodState(3,false), woodState(3,true))`。
    fn default() -> Self {
        Self {
            base: TreeRTG {
                log_block: block_rules::wood_state(3, false),
                leaves_block: block_rules::wood_state(3, true),
                ..TreeRTG::default()
            },
            base_height: 10,
            extra_random_height: 27,
        }
    }
}

impl Tree for TreeMegaJungle {
    fn furthest_likely_extension(&self) -> i32 {
        8
    }

    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(ext * ext) / 16.0)
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        let wood = self.base.log_block;
        let leaves = self.base.leaves_block;
        let height = self.get_height(rand);

        if !self.ensure_growable(world, pos, height) {
            return false;
        }

        // 树冠
        self.create_crown(world, pos.up_n(height), 2, leaves);

        // 侧枝
        let mut j = (pos.y + height - 2) - rand.next_int_bound(4);
        let half = pos.y + height / 2;
        while j > half {
            let angle = f(rand.next_float() as f64 * f(PI * 2.0));
            let cos_a = f(angle.cos());
            let sin_a = f(angle.sin());
            let mut k = pos.x;
            let mut l = pos.z;
            for i1 in 0..5 {
                k = pos.x + f(1.5 + f(cos_a * f(i1 as f64))) as i32;
                l = pos.z + f(1.5 + f(sin_a * f(i1 as f64))) as i32;
                world.set_block_state(k, j - 3 + i1 / 2, l, wood);
            }
            let j2 = 1 + rand.next_int_bound(2);
            for k1 in (j - j2)..=j {
                let l1 = k1 - j;
                self.grow_leaves_layer(world, Pos::new(k, k1, l), 1 - l1, leaves);
            }
            j -= 2 + rand.next_int_bound(4);
        }

        // 2x2 主干 + 藤蔓
        for i2 in 0..height {
            let trunk = pos.up_n(i2);
            if can_grow_into(world.block(trunk.x, trunk.y, trunk.z)) {
                world.set_block_state(trunk.x, trunk.y, trunk.z, wood);
                if i2 > 0 {
                    self.place_vine(world, rand, trunk.west(), "east");
                    self.place_vine(world, rand, trunk.north(), "south");
                }
            }
            if i2 < height - 1 {
                let east = trunk.east();
                if can_grow_into(world.block(east.x, east.y, east.z)) {
                    world.set_block_state(east.x, east.y, east.z, wood);
                    if i2 > 0 {
                        self.place_vine(world, rand, east.east(), "west");
                        self.place_vine(world, rand, east.north(), "south");
                    }
                }
                let se = east.south();
                if can_grow_into(world.block(se.x, se.y, se.z)) {
                    world.set_block_state(se.x, se.y, se.z, wood);
                    if i2 > 0 {
                        self.place_vine(world, rand, se.east(), "west");
                        self.place_vine(world, rand, se.south(), "north");
                    }
                }
                let south = trunk.south();
                if can_grow_into(world.block(south.x, south.y, south.z)) {
                    world.set_block_state(south.x, south.y, south.z, wood);
                    if i2 > 0 {
                        self.place_vine(world, rand, south.west(), "east");
                        self.place_vine(world, rand, south.south(), "north");
                    }
                }
            }
        }
        true
    }
}

impl TreeMegaJungle {
    /// 对应 `getHeight`。
    fn get_height(&self, rand: &mut JavaRandom) -> i32 {
        let mut h = rand.next_int_bound(3) + self.base_height;
        if self.extra_random_height > 1 {
            h += rand.next_int_bound(self.extra_random_height);
        }
        h
    }

    /// 对应 `ensureGrowable`。
    fn ensure_growable(&self, world: &mut GenWorld, pos: Pos, height: i32) -> bool {
        self.is_space_at(world, pos, height) && self.ensure_dirs_underneath(world, pos)
    }

    /// 对应 `isSpaceAt`：2x2 树干所需空间是否为空。
    fn is_space_at(&self, world: &GenWorld, pos: Pos, height: i32) -> bool {
        if pos.y < 1 || pos.y + height + 1 > 256 {
            return false;
        }
        for i in 0..=(1 + height) {
            let mut radius = 2;
            if i == 0 {
                radius = 1;
            } else if i >= (1 + height) - 2 {
                radius = 2;
            }
            for k in -radius..=radius {
                for l in -radius..=radius {
                    let y = pos.y + i;
                    if y < 0 || y >= 256 {
                        return false;
                    }
                    if !can_grow_into(world.block(pos.x + k, y, pos.z + l)) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// 对应 `ensureDirtsUnderneath`：2x2 地面必须是草/土，并改成泥土。
    fn ensure_dirs_underneath(&self, world: &mut GenWorld, pos: Pos) -> bool {
        let ground = pos.down();
        let block = world.block(ground.x, ground.y, ground.z);
        if (block == 2 || block == 3) && pos.y >= 2 {
            world.set_block_state(ground.x, ground.y, ground.z, 48);
            world.set_block_state(ground.x + 1, ground.y, ground.z, 48);
            world.set_block_state(ground.x, ground.y, ground.z + 1, 48);
            world.set_block_state(ground.x + 1, ground.y, ground.z + 1, 48);
            true
        } else {
            false
        }
    }

    /// 对应 `createCrown`。
    fn create_crown(&self, world: &mut GenWorld, pos: Pos, radius: i32, leaves: i32) {
        for j in -2..=0 {
            self.grow_leaves_layer_strict(world, pos.up_n(j), (radius + 1) - j, leaves);
        }
    }

    /// 对应 `growLeavesLayerStrict`：圆形树叶（四角放宽）。
    fn grow_leaves_layer_strict(
        &self,
        world: &mut GenWorld,
        center: Pos,
        width: i32,
        leaves: i32,
    ) {
        let r2 = width * width;
        for j in -width..=(width + 1) {
            for k in -width..=(width + 1) {
                let l = j - 1;
                let l1 = k - 1;
                let in_circle = (j * j + k * k <= r2)
                    || (l * l + l1 * l1 <= r2)
                    || (j * j + l1 * l1 <= r2)
                    || (l * l + k * k <= r2);
                if in_circle {
                    let p = Pos::new(center.x + j, center.y, center.z + k);
                    let state = world.block_state(p.x, p.y, p.z);
                    if matches!(
                        block_rules::material(state),
                        block_rules::AIR | block_rules::LEAVES
                    ) {
                        world.set_block_state(p.x, p.y, p.z, leaves);
                    }
                }
            }
        }
    }

    /// 对应 `growLeavesLayer`：圆形树叶。
    fn grow_leaves_layer(&self, world: &mut GenWorld, center: Pos, width: i32, leaves: i32) {
        if width < 0 {
            return;
        }
        let r2 = width * width;
        for j in -width..=width {
            for k in -width..=width {
                if j * j + k * k <= r2 {
                    let p = Pos::new(center.x + j, center.y, center.z + k);
                    let state = world.block_state(p.x, p.y, p.z);
                    if matches!(
                        block_rules::material(state),
                        block_rules::AIR | block_rules::LEAVES
                    ) {
                        world.set_block_state(p.x, p.y, p.z, leaves);
                    }
                }
            }
        }
    }

    /// 对应 `placeVine`：1/3 概率不放藤蔓，必须是空气。
    fn place_vine(&self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, face: &str) {
        if rand.next_int_bound(3) > 0 && world.is_air(pos.x, pos.y, pos.z) {
            world.set_block_state(pos.x, pos.y, pos.z, block_rules::vine_with_face(face));
        }
    }
}

/// 对应 `WorldGenAbstractTree::canGrowInto`。
fn can_grow_into(block_id: u8) -> bool {
    let id = block_id as i32;
    matches!(
        block_rules::material(block_rules::default_state(id)),
        block_rules::AIR | block_rules::LEAVES
    ) || matches!(id, 2 | 3 | 17 | 162 | 6 | 106)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use std::collections::HashMap;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -2..=2 {
            for chunk_z in -2..=2 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -32..48 {
            for z in -32..48 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 32 } else { 48 });
                }
            }
        }
        world
    }

    #[test]
    fn mega_jungle_grows_2x2_trunk_and_crown() {
        let mut world = flat_world();
        let mut tree = TreeMegaJungle::default();
        let mut rand = JavaRandom::new(11);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -32..48 {
            for z in -32..48 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        17 => logs += 1,
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // 2x2 树干至少 10 高 → 至少 40 根原木
        assert!(logs >= 40, "巨型丛林树原木太少：{logs}");
        // 树冠 + 侧枝树叶
        assert!(leaves > 30, "巨型丛林树树叶太少：{leaves}");
    }
}
