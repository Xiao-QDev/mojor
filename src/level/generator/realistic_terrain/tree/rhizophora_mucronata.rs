//! 对应 `rtg/tree/TreeRTGRhizophoraMucronata.php`：红树林（丛林边缘用）。
//!
//! 结构：笔直主干（trunkSize=15），顶端向 3~4 个水平方向各伸出一根短树枝
//! （c 从 1.0 累加到 3.0，步长 0.25），每根树枝末端和中心各放一簇
//! 3x3 树叶。方向从 0° 开始均匀分布并加 ±10° 随机偏移。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct TreeRhizophoraMucronata {
    pub base: TreeRTG,
}

impl Default for TreeRhizophoraMucronata {
    /// 对应 `constructTreeRTGRhizophoraMucronata0`：默认丛林木材。
    fn default() -> Self {
        let base = TreeRTG {
            log_block: block_rules::wood_state(3, false),
            leaves_block: block_rules::wood_state(3, true),
            trunk_size: 15,
            crown_size: 4,
            ..TreeRTG::default()
        };
        Self { base }
    }
}

impl Tree for TreeRhizophoraMucronata {
    fn furthest_likely_extension(&self) -> i32 {
        5
    }

    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(ext * ext) / 32.0)
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n2(world, pos) {
            return false;
        }
        let (x, y, z) = (pos.x, pos.y, pos.z);
        let trunk_size = self.base.trunk_size;
        let log_block = self.base.log_block;
        let leaves_block = self.base.leaves_block;
        let no_leaves = self.base.no_leaves;

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        for i in 0..trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }

        let top_y = y + trunk_size;
        let branches = 3 + rand.next_int_bound(2);
        let increment = 360.0 / branches as f64;
        let mut direction = f(rand.next_int_bound(21) as f64 - 10.0);

        for _ in 0..branches {
            let xd = f((direction * PI / 180.0).cos());
            let yd = f((direction * PI / 180.0).sin());
            let mut c = f(1.0);
            while c < f(3.0) {
                let bx = x + f(xd * c) as i32;
                let bz = z + f(yd * c) as i32;
                self.base
                    .place_log_block(world, &mut tracker, Pos::new(bx, top_y, bz), log_block);
                c = f(c + 0.25);
            }
            let ex = x + f(xd * c) as i32;
            let ez = z + f(yd * c) as i32;
            Self::leaf_cluster(
                &self.base, world, &mut tracker, ex, top_y, ez, leaves_block, no_leaves,
            );
            direction = f(direction + increment);
        }

        self.base
            .place_log_block(world, &mut tracker, Pos::new(x, top_y, z), log_block);
        Self::leaf_cluster(
            &self.base, world, &mut tracker, x, top_y, z, leaves_block, no_leaves,
        );
        true
    }
}

impl TreeRhizophoraMucronata {
    /// 对应 `leafCluster`：y+1 层 3x3 满铺 + y 层 3x3 十字（去角）。
    fn leaf_cluster(
        base: &TreeRTG,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        x: i32,
        y: i32,
        z: i32,
        leaves_block: i32,
        no_leaves: bool,
    ) {
        if !no_leaves {
            for i in -1i32..=1 {
                for j in -1i32..=1 {
                    base.place_leaves_block(
                        world,
                        tracker,
                        Pos::new(x + i, y + 1, z + j),
                        leaves_block,
                    );
                }
            }
            for i in -1i32..=1 {
                for j in -1i32..=1 {
                    if i.abs() + j.abs() < 2 {
                        base.place_leaves_block(
                            world,
                            tracker,
                            Pos::new(x + i, y, z + j),
                            leaves_block,
                        );
                    }
                }
            }
        }
    }
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
    fn mangrove_grows_trunk_branches_and_leaves() {
        let mut world = flat_world();
        let mut tree = TreeRhizophoraMucronata::default();
        tree.base.trunk_size = 15;
        let mut rand = JavaRandom::new(5);
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
        // 主干 15 根 + 3~4 根水平树枝
        assert!(logs > 15, "红树林原木太少：{logs}");
        // 每根树枝末端 + 中心各一簇 3x3 树叶
        assert!(leaves > 10, "红树林树叶太少：{leaves}");
    }

    #[test]
    fn mangrove_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeRhizophoraMucronata::default();
        let mut rand = JavaRandom::new(5);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
