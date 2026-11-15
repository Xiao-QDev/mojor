//! 对应 `rtg/tree/TreeRTGPinusNigra.php`：黑松（极端丘陵用）。
//!
//! 结构：主干 + 每节向随机方向伸出的树枝 + 球形树叶 + 向下延伸的树根。
//! 树枝长度从 1 逐层增加 0.25，方向每层偏转 90..270 度。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct TreePinusNigra {
    pub base: TreeRTG,
}

impl Default for TreePinusNigra {
    fn default() -> Self {
        Self {
            base: TreeRTG::default(),
        }
    }
}

impl Tree for TreePinusNigra {
    /// 对应 `estimatedSize`：branchLength 固定 4.0，`f(4*4/16) = f(1.0) = 1.0`。
    fn estimated_size(&self) -> f64 {
        let branch_length = f(4.0);
        f(f(branch_length * branch_length) / 16.0)
    }

    /// 对应 `furthestLikelyExtension`：`1 + 4 = 5`。
    fn furthest_likely_extension(&self) -> i32 {
        5
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n2(world, pos) {
            return false;
        }
        let (x, y, z) = (pos.x, pos.y, pos.z);
        let trunk_log = self.base.get_trunk_log(self.base.log_block);
        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        let height = self.base.trunk_size;
        let leafheight = self.base.crown_size;
        let branch_increase = f(0.25);

        // 主干：y .. y+height（共 height+1 根）
        for i in 0..=height {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }

        // 顶端树叶球
        self.build_leaves(world, rand, x, y + height, z, 2, &mut tracker);

        // 树根：向 5 个方向（含自身）向下延伸，遇泥土停止
        self.build_trunk(world, rand, x, y, z, &mut tracker);

        // 逐层向下放树枝
        let mut bl = f(1.0);
        let mut dir: i32 = 0;
        // j 从 height 降到 height - leafheight（含两端）
        let j_start = height;
        let j_end = height - leafheight;
        let mut j = j_start;
        while j >= j_end {
            bl = f(bl + branch_increase);
            dir += rand.next_int_bound(180) + 90;
            if dir > 360 {
                dir -= 360;
            }
            let angle = (dir as f64 * PI) / 180.0;
            let xd = f(angle.cos());
            let yd = f(angle.sin());

            // 放树枝原木
            let mut b = 0;
            while f(b as f64) <= bl {
                let bx = x + f(f(b as f64) * xd) as i32;
                let bz = z + f(f(b as f64) * yd) as i32;
                self.base
                    .place_log_block(world, &mut tracker, Pos::new(bx, y + j, bz), trunk_log);
                b += 1;
            }

            // 树干处树叶球 + 树枝末端树叶球（末端用跳出循环后的 b）
            self.build_leaves(world, rand, x, y + j, z, 2, &mut tracker);
            let bx = x + f(f(b as f64) * xd) as i32;
            let bz = z + f(f(b as f64) * yd) as i32;
            self.build_leaves(world, rand, bx, y + j, bz, 2, &mut tracker);

            j -= 1;
        }
        true
    }
}

impl TreePinusNigra {
    /// 对应 `buildLeaves`：以 (x,y,z) 为中心、半径 size 的球内放树叶。
    /// 球心一半体积（l < t/2）必放，其余 50% 概率。
    fn build_leaves(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        size: i32,
        tracker: &mut SkylightTracker,
    ) {
        if self.base.no_leaves {
            return;
        }
        let t = size * size;
        let half_t = t / 2;
        for i in -size..=size {
            for j in -size..=size {
                for k in -size..=size {
                    let l = i * i + j * j + k * k;
                    if l <= t {
                        let always = l < half_t;
                        if always || rand.next_boolean() {
                            self.base.place_leaves_block(
                                world,
                                tracker,
                                Pos::new(x + i, y + j, z + k),
                                self.base.leaves_block,
                            );
                        }
                    }
                }
            }
        }
    }

    /// 对应 `buildTrunk`：向 5 个方向（含正下方）向下延伸树根，遇泥土停止。
    fn build_trunk(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
    ) {
        let pos: [(i32, i32); 5] = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)];
        for &(dx, dz) in &pos {
            let mut sh = rand.next_int_bound(3) + y;
            while sh > y - 3 {
                if world.block_state(x + dx, sh, z + dz) == 48 {
                    break;
                }
                self.base
                    .place_trunk_block(world, tracker, Pos::new(x + dx, sh, z + dz));
                sh -= 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

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
    fn nigra_grows_trunk_branches_and_leaves() {
        let mut world = flat_world();
        let mut tree = TreePinusNigra::default();
        tree.base.trunk_size = 8;
        tree.base.crown_size = 6;
        let mut rand = JavaRandom::new(13);
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
        // 主干 9 根 + 每节树枝原木 + 顶部，应远多于 9
        assert!(logs > 9, "黑松原木太少：{logs}");
        // 每节两层树叶球（树干+树枝末），应有大量树叶
        assert!(leaves > 50, "黑松树叶太少：{leaves}");
    }

    #[test]
    fn nigra_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreePinusNigra::default();
        let mut rand = JavaRandom::new(13);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
