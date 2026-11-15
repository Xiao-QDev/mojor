//! 对应 `rtg/tree/TreeRTGPiceaPungens.php`：蓝叶云杉（巨型针叶林用）。
//!
//! 结构比 PiceaSitchensis 简单：纯主干 + 两段锥形树冠（大段半径 2、小段半径 1），
//! 没有向侧面延伸的树枝。`buildBranch`/`buildLeaves` 在 PHP 的 generate 里未被调用，
//! 这里只保留 generate 直接落方块的逻辑。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreePiceaPungens {
    pub base: TreeRTG,
}

impl Default for TreePiceaPungens {
    fn default() -> Self {
        let materials = TreeMaterials::SPRUCE;
        Self {
            base: TreeRTG {
                log_block: materials.log,
                leaves_block: materials.leaves,
                branch_block: materials.branches,
                trunk_size: 4,
                crown_size: 6,
                ..TreeRTG::default()
            },
        }
    }
}

impl Tree for TreePiceaPungens {
    /// 对应 `estimatedSize`：branchLength 固定 3.0，`f(3*3 / 16)`。
    fn estimated_size(&self) -> f64 {
        let branch_length = f(3.0);
        f(f(branch_length * branch_length) / 16.0)
    }

    /// 对应 `furthestLikelyExtension`：`1 + 3 = 4`。
    fn furthest_likely_extension(&self) -> i32 {
        4
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n2(world, pos) {
            return false;
        }
        let (x, mut y, z) = (pos.x, pos.y, pos.z);
        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        // `small = ceil(intdiv(crownSize, 2))` = crownSize / 2（正数整除即 floor）
        let small = self.base.crown_size / 2;
        let large = small;

        for _ in 0..self.base.trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y, z));
            y += 1;
        }

        // 大段：半径 2 的菱形树冠
        for _ in 0..large {
            if !self.base.no_leaves {
                for j in -2_i32..=2 {
                    for k in -2_i32..=2 {
                        let manhattan = j.abs() + k.abs();
                        let inside = (j > -2) && (k > -2) && (j < 2) && (k < 2);
                        if manhattan != 4 && (inside || rand.next_int_bound(4) != 0) {
                            self.base.place_leaves_block(
                                world,
                                &mut tracker,
                                Pos::new(x + j, y, z + k),
                                self.base.leaves_block,
                            );
                        }
                    }
                }
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
            y += 1;
        }

        // 小段：半径 1 的菱形树冠，第一层额外向四正方向延伸两格
        for i in 0..small {
            if !self.base.no_leaves {
                for j in -1_i32..=1 {
                    for k in -1_i32..=1 {
                        if (j.abs() + k.abs() < 2) || rand.next_int_bound(4) != 0 {
                            self.base.place_leaves_block(
                                world,
                                &mut tracker,
                                Pos::new(x + j, y, z + k),
                                self.base.leaves_block,
                            );
                        }
                    }
                }
            }
            if i == 0 {
                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2)] {
                    self.base.place_leaves_block(
                        world,
                        &mut tracker,
                        Pos::new(x + dx, y, z + dz),
                        self.base.leaves_block,
                    );
                }
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
            y += 1;
        }

        // 顶部收顶：一根原木 + 四向树叶 + 上方两格树叶
        self.base
            .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
        if !self.base.no_leaves {
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                self.base.place_leaves_block(
                    world,
                    &mut tracker,
                    Pos::new(x + dx, y, z + dz),
                    self.base.leaves_block,
                );
            }
            self.base
                .place_leaves_block(world, &mut tracker, Pos::new(x, y + 1, z), self.base.leaves_block);
            self.base
                .place_leaves_block(world, &mut tracker, Pos::new(x, y + 2, z), self.base.leaves_block);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -1..=1 {
            for chunk_z in -1..=1 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -16..32 {
            for z in -16..32 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 32 } else { 48 });
                }
            }
        }
        world
    }

    #[test]
    fn pungens_grows_conical_crown_and_top_leaves() {
        let mut world = flat_world();
        let mut tree = TreePiceaPungens::default();
        tree.base.trunk_size = 4;
        tree.base.crown_size = 6;
        let mut rand = JavaRandom::new(11);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -16..32 {
            for z in -16..32 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        17 => logs += 1,
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // trunk(4) + large段(3) + small段(3) + 顶部(1) = 11 根原木
        assert_eq!(logs, 11, "蓝叶云杉原木数不对：{logs}");
        // 大段 3 层半径 2 菱形 + 小段 3 层半径 1 菱形 + 第一层四向延伸 + 顶部
        assert!(leaves > 40, "蓝叶云杉树叶太少：{leaves}");
    }

    #[test]
    fn pungens_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreePiceaPungens::default();
        let mut rand = JavaRandom::new(11);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
