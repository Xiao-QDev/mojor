//! 对应 `rtg/tree/TreeRTGAcaciaBucheri.php`：金合欢（稀树草原用）。
//!
//! 结构：笔直主干 + 从树冠段向随机水平方向伸出的 2~3 根对角树枝，
//! 每根树枝末端和主干顶端各放一团 5x5 的树叶。树枝沿 `(xd, yd)` 方向
//! 每上升一格就往外多走 0.5 格（`c` 累加），落点用 `f()` 截断到 float32。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct TreeAcaciaBucheri {
    pub base: TreeRTG,
}

impl Default for TreeAcaciaBucheri {
    /// 对应 `constructTreeRTGAcaciaBucheri0`。
    fn default() -> Self {
        let materials = TreeMaterials::ACACIA;
        let base = TreeRTG {
            log_block: materials.log,
            leaves_block: materials.leaves,
            branch_block: materials.branches,
            trunk_size: 10,
            lowest_variable_trunk_proportion: f(0.44999998807907104),
            trunk_proportion_variability: f(0.20000000298023224),
            max_allowed_obstruction: 4,
            ..TreeRTG::default()
        };
        Self { base }
    }
}

impl Tree for TreeAcaciaBucheri {
    /// 对应 `furthestLikelyExtension`：`crownSize + 1`。
    fn furthest_likely_extension(&self) -> i32 {
        self.base.crown_size + 1
    }

    /// 对应 `estimatedSize`：`f(furthest^2 / 32.0)`。
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
        let crown_size = self.base.crown_size;
        let log_block = self.base.log_block;
        let leaves_block = self.base.leaves_block;
        let no_leaves = self.base.no_leaves;

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        // 主干：y .. y+trunkSize+crownSize-1
        for i in 0..(trunk_size + crown_size) {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }

        // 顶端树叶
        self.gen_leaves(
            world,
            &mut tracker,
            x,
            y + trunk_size + crown_size,
            z,
            log_block,
            leaves_block,
            no_leaves,
        );

        // 对角树枝：2~3 根
        let mut dir = rand.next_int_bound(360);
        let branch_count = 1 + rand.next_int_bound(2);
        for _ in 0..branch_count {
            let below_top = 1;
            let mut sh = trunk_size + rand.next_int_bound(3);
            let angle = f((dir as f64 * PI) / 180.0);
            let xd = f(f(angle.cos()) * 2.0);
            let yd = f(f(angle.sin()) * 2.0);
            let mut c = f(1.0);

            while sh < (trunk_size + crown_size - below_top) {
                let bx = x + f(xd * c) as i32;
                let bz = z + f(yd * c) as i32;
                self.base.place_log_block(
                    world,
                    &mut tracker,
                    Pos::new(bx, y + sh, bz),
                    log_block,
                );
                sh += 1;
                c = f(c + 0.5);
            }

            dir = dir + (50 + rand.next_int_bound(172));
            let lx = x + f(xd * c) as i32;
            let lz = z + f(yd * c) as i32;
            self.gen_leaves(
                world,
                &mut tracker,
                lx,
                y + sh,
                lz,
                log_block,
                leaves_block,
                no_leaves,
            );
        }
        true
    }
}

impl TreeAcaciaBucheri {
    /// 对应 `genLeaves`：顶端 3x3 一层 + 下方 5x5 菱形一层，中心放原木。
    fn gen_leaves(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        x: i32,
        y: i32,
        z: i32,
        log_block: i32,
        leaves_block: i32,
        no_leaves: bool,
    ) {
        if !no_leaves {
            for i in -1i32..=1 {
                for j in -1i32..=1 {
                    self.base.place_leaves_block(
                        world,
                        tracker,
                        Pos::new(x + i, y + 1, z + j),
                        leaves_block,
                    );
                }
            }
            for i in -2i32..=2 {
                for j in -2i32..=2 {
                    if i.abs() + j.abs() < 4 {
                        self.base.place_leaves_block(
                            world,
                            tracker,
                            Pos::new(x + i, y, z + j),
                            leaves_block,
                        );
                    }
                }
            }
        }
        self.base
            .place_log_block(world, tracker, Pos::new(x, y, z), log_block);
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
    fn acacia_grows_trunk_diagonal_branches_and_leaves() {
        let mut world = flat_world();
        let mut tree = TreeAcaciaBucheri::default();
        tree.base.trunk_size = 10;
        tree.base.crown_size = 4;
        let mut rand = JavaRandom::new(7);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -32..48 {
            for z in -32..48 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        162 => logs += 1,
                        161 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // 主干 14 根 + 对角树枝 + 树叶团中心原木
        assert!(logs > 14, "金合欢原木太少：{logs}");
        // 每团 3x3 + 5x5 菱形树叶，2~3 团加顶端
        assert!(leaves > 30, "金合欢树叶太少：{leaves}");
    }

    #[test]
    fn acacia_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeAcaciaBucheri::default();
        let mut rand = JavaRandom::new(7);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
