//! 对应 `rtg/tree/TreeRTGVachelliaTortilis{,Mega}.php` + `*Generation.php`：
//! 伞刺金合欢（稀树草原变量树用）。
//!
//! 两种树共享"主干 + 从树冠段向四周辐射的树枝 + 树枝末端树叶团"的结构，
//! 区别只是尺寸和分叉数：普通版 3~4 根主枝，Mega 版 2~3 根主枝且每根主枝
//! 再分 2~3 根叶枝。所有方向和长度都经过 `f()` 截断。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::branch::{BranchVector, TreeBranch};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

/// 对应 `TreeRTGVachelliaTortilis`：小型伞刺金合欢。
#[derive(Clone, Debug)]
pub struct TreeVachelliaTortilis {
    pub base: TreeRTG,
}

impl Default for TreeVachelliaTortilis {
    /// 对应 `constructTreeRTGVachelliaTortilis0`。
    fn default() -> Self {
        let materials = TreeMaterials::ACACIA;
        let base = TreeRTG {
            log_block: materials.log,
            leaves_block: materials.leaves,
            branch_block: materials.branches,
            lowest_variable_trunk_proportion: f(0.5),
            trunk_proportion_variability: f(0.20000000298023224),
            ..TreeRTG::default()
        };
        Self { base }
    }
}

impl Tree for TreeVachelliaTortilis {
    fn furthest_likely_extension(&self) -> i32 {
        8
    }

    /// 对应 `estimatedSize`：`parent::estimatedSize() / 2.0`。
    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(f(ext * ext) / 16.0) / 2.0)
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
        let branch_block = self.base.branch_block;
        let log_block = self.base.log_block;
        let leaves_block = self.base.leaves_block;
        let no_leaves = self.base.no_leaves;
        let generate_flag = self.base.generate_flag;

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        // 主干：y .. y+trunkSize（共 trunkSize+1 根）
        for i in 0..=trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }

        let branch_count = 3 + rand.next_int_bound(2);
        let base_dir = f(f(rand.next_float() as f64 * PI) * 2.0);
        let increment = f(f(f(PI * 2.0) / f(branch_count as f64)));
        let variability = f(f(increment) / f(4.0));
        let mut direction = f(base_dir - f(f(variability) / f(2.0)));

        for branch_number in 0..branch_count {
            let dir = f(direction + f(rand.next_float() as f64 * variability));
            Self::make_branch(
                &self.base,
                world,
                &mut tracker,
                rand,
                pos,
                trunk_size,
                crown_size,
                branch_block,
                log_block,
                leaves_block,
                no_leaves,
                generate_flag,
                dir,
                branch_number == 0,
            );
            direction = f(direction + increment);
        }
        true
    }
}

impl TreeVachelliaTortilis {
    /// 对应 `TreeRTGVachelliaTortilisGeneration::makeBranch`。
    #[allow(clippy::too_many_arguments)]
    fn make_branch(
        base: &TreeRTG,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        rand: &mut JavaRandom,
        start: Pos,
        trunk_size: i32,
        crown_size: i32,
        branch_block: i32,
        log_block: i32,
        leaves_block: i32,
        no_leaves: bool,
        generate_flag: i32,
        direction: f64,
        back_branch: bool,
    ) {
        let mut ascent = f((crown_size - 1) as f64 - rand.next_int_bound(2) as f64);
        if ascent < f(0.0) {
            ascent = f(0.0);
        }
        let length = f((4 + rand.next_int_bound(3)) as f64);
        let vertical_shift = f(f(ascent) / f(length));
        let effective_length = f(f(f(length * length) + f(ascent * ascent)).sqrt());
        let split_start = start.up_n(trunk_size);

        let mut branch = TreeBranch::new(direction, vertical_shift, effective_length, 1, split_start);
        while branch.not_done() {
            let loc = branch.moved();
            tracker.test_place(world, loc, branch_block, generate_flag);
        }
        let branch_end = branch.location();
        gen_leaves_n4(
            base,
            world,
            tracker,
            branch_end.x,
            branch_end.y,
            branch_end.z,
            true,
            log_block,
            leaves_block,
            no_leaves,
        );

        if back_branch {
            let new_x = (split_start.x + branch_end.x) / 2;
            let new_y = (split_start.y + branch_end.y) / 2;
            let new_z = (split_start.z + branch_end.z) / 2;
            let top_ascent = f(((split_start.y + crown_size) - new_y + 1) as f64);
            let top_length = f(
                f(f(f(length * length) / 4.0) + f(f(top_ascent * top_ascent) / 2.0)).sqrt(),
            );
            let top_dir = f(
                f(f(direction + PI) - 0.20000000298023224) + f(rand.next_float() as f64 * 0.4000000059604645),
            );
            let top_vs = f(f(top_ascent) / f(top_length));
            let mut top_branch = TreeBranch::new(
                top_dir,
                top_vs,
                top_length,
                1,
                Pos::new(new_x, new_y, new_z),
            );
            while top_branch.not_done() {
                let loc = top_branch.moved();
                tracker.test_place(world, loc, branch_block, generate_flag);
            }
            let top_end = top_branch.location();
            gen_leaves_n4(
                base,
                world,
                tracker,
                top_end.x,
                top_end.y,
                top_end.z,
                true,
                log_block,
                leaves_block,
                no_leaves,
            );
        }
    }
}

/// 对应 `TreeRTGVachelliaTortilisMega`：大型伞刺金合欢。
#[derive(Clone, Debug)]
pub struct TreeVachelliaTortilisMega {
    pub base: TreeRTG,
}

impl Default for TreeVachelliaTortilisMega {
    /// 对应 `constructTreeRTGVachelliaTortilisMega0`。
    fn default() -> Self {
        let materials = TreeMaterials::ACACIA;
        let base = TreeRTG {
            log_block: materials.log,
            leaves_block: materials.leaves,
            branch_block: materials.branches,
            lowest_variable_trunk_proportion: f(0.30000001192092896),
            trunk_proportion_variability: f(0.20000000298023224),
            ..TreeRTG::default()
        };
        Self { base }
    }
}

impl Tree for TreeVachelliaTortilisMega {
    fn furthest_likely_extension(&self) -> i32 {
        14
    }

    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(f(ext * ext) / 16.0) / 2.0)
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
        let branch_block = self.base.branch_block;
        let log_block = self.base.log_block;
        let leaves_block = self.base.leaves_block;
        let no_leaves = self.base.no_leaves;
        let generate_flag = self.base.generate_flag;

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        for i in 0..=trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }

        let branch_count = 2 + rand.next_int_bound(2);
        let base_dir = f(f(rand.next_float() as f64 * PI) * 2.0);
        let increment = f(f(f(PI * 2.0) / f(branch_count as f64)));
        let variability = f(f(increment) / f(4.0));
        let mut direction = f(base_dir - f(f(variability) / f(2.0)));
        let wide = branch_count > 2;

        for _ in 0..branch_count {
            let dir = f(direction + f(rand.next_float() as f64 * variability));
            Self::make_trunk_branch(
                &self.base,
                world,
                &mut tracker,
                rand,
                pos,
                trunk_size,
                crown_size,
                branch_block,
                log_block,
                leaves_block,
                no_leaves,
                generate_flag,
                dir,
                wide,
            );
            direction = f(direction + increment);
        }
        true
    }
}

impl TreeVachelliaTortilisMega {
    /// 对应 `TreeRTGVachelliaTortilisMegaGeneration::makeTrunkBranch`。
    #[allow(clippy::too_many_arguments)]
    fn make_trunk_branch(
        base: &TreeRTG,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        rand: &mut JavaRandom,
        trunk_base: Pos,
        trunk_size: i32,
        crown_size: i32,
        branch_block: i32,
        log_block: i32,
        leaves_block: i32,
        no_leaves: bool,
        generate_flag: i32,
        direction: f64,
        wide: bool,
    ) {
        let mut ascent = f((crown_size / 2 - rand.next_int_bound(2)) as f64);
        if ascent < f(0.0) {
            ascent = f(0.0);
        }
        let mut length = f((3 + rand.next_int_bound(3)) as f64);
        if wide {
            length = f(f(length) + 2.0);
        }
        let vertical_shift = f(f(ascent) / f(length));
        let effective_length = f(f(f(length * length) + f(ascent * ascent)).sqrt());
        let split_start = trunk_base.up_n(trunk_size);

        let mut branch = TreeBranch::new(direction, vertical_shift, effective_length, 1, split_start);
        while branch.not_done() {
            let loc = branch.moved();
            tracker.test_place(world, loc, branch_block, generate_flag);
        }
        let branch_end = branch.location();

        // 二级叶枝
        let sub_count = 2 + rand.next_int_bound(2);
        let sub_base_dir = f(f(rand.next_float() as f64 * PI) * 2.0);
        let sub_increment = f(f(f(PI * 2.0) / f(sub_count as f64)));
        let sub_variability = f(f(sub_increment) / f(4.0));
        let mut sub_direction = f(sub_base_dir - f(f(sub_variability) / f(2.0)));

        for _ in 0..sub_count {
            let sub_dir = f(sub_direction + f(rand.next_float() as f64 * sub_variability));
            Self::make_leaf_branch(
                base,
                world,
                tracker,
                rand,
                trunk_base,
                trunk_size,
                crown_size,
                branch_block,
                log_block,
                leaves_block,
                no_leaves,
                generate_flag,
                sub_dir,
                branch_end,
            );
            sub_direction = f(sub_direction + sub_increment);
        }
    }

    /// 对应 `makeLeafBranch`。
    #[allow(clippy::too_many_arguments)]
    fn make_leaf_branch(
        base: &TreeRTG,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        rand: &mut JavaRandom,
        trunk_base: Pos,
        trunk_size: i32,
        crown_size: i32,
        branch_block: i32,
        log_block: i32,
        leaves_block: i32,
        no_leaves: bool,
        generate_flag: i32,
        direction: f64,
        branch_start: Pos,
    ) {
        let length = f((3 + rand.next_int_bound(3)) as f64);
        let distance_out = {
            let horizontal = BranchVector::new(direction, 0.0);
            let end = horizontal.reposition(branch_start, length);
            let x_dist = (trunk_base.x - end.x) as f64;
            let z_dist = (trunk_base.z - end.z) as f64;
            f(f(x_dist * x_dist + z_dist * z_dist).sqrt())
        };
        let remaining_height =
            ((trunk_size + crown_size) + trunk_base.y - branch_start.y) as f64;
        let mut ascent = f(f(remaining_height - rand.next_int_bound(3) as f64) - f(f(distance_out) / 4.0));
        if ascent < f(0.0) {
            ascent = f(0.0);
        }
        let vertical_shift = f(f(ascent) / f(length));
        let effective_length = f(f(f(length * length) + f(ascent * ascent)).sqrt());

        let mut branch = TreeBranch::new(direction, vertical_shift, effective_length, 1, branch_start);
        while branch.not_done() {
            let loc = branch.moved();
            tracker.test_place(world, loc, branch_block, generate_flag);
        }
        let end = branch.location();
        gen_leaves_n4(
            base,
            world,
            tracker,
            end.x,
            end.y,
            end.z,
            true,
            log_block,
            leaves_block,
            no_leaves,
        );
    }
}

/// 对应 `genLeavesN4`：上下两层菱形树叶，中心放原木。
fn gen_leaves_n4(
    base: &TreeRTG,
    world: &mut GenWorld,
    tracker: &mut SkylightTracker,
    x: i32,
    y: i32,
    z: i32,
    bigger: bool,
    log_block: i32,
    leaves_block: i32,
    no_leaves: bool,
) {
    if !no_leaves {
        let mut top_radius: i32 = 1;
        let mut bottom_radius: i32 = 2;
        if bigger {
            top_radius += 1;
            bottom_radius += 1;
        }
        for i in -top_radius..=top_radius {
            for j in -top_radius..=top_radius {
                if i.abs() + j.abs() < top_radius + 2 {
                    base.place_leaves_block(
                        world,
                        tracker,
                        Pos::new(x + i, y + 1, z + j),
                        leaves_block,
                    );
                }
            }
        }
        for i in -bottom_radius..=bottom_radius {
            for j in -bottom_radius..=bottom_radius {
                if i.abs() + j.abs() < bottom_radius + 2 {
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
    base.place_log_block(world, tracker, Pos::new(x, y, z), log_block);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -3..=3 {
            for chunk_z in -3..=3 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -48..64 {
            for z in -48..64 {
                for y in 60..=63 {
                    world.set_block_state(x, y, z, if y == 63 { 32 } else { 48 });
                }
            }
        }
        world
    }

    #[test]
    fn tortilis_grows_trunk_and_radial_branches() {
        let mut world = flat_world();
        let mut tree = TreeVachelliaTortilis::default();
        tree.base.trunk_size = 6;
        tree.base.crown_size = 5;
        let mut rand = JavaRandom::new(21);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -48..64 {
            for z in -48..64 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        162 => logs += 1,
                        161 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // 主干 7 根 + 3~4 根辐射树枝 + 回枝
        assert!(logs > 7, "伞刺金合欢原木太少：{logs}");
        assert!(leaves > 20, "伞刺金合欢树叶太少：{leaves}");
    }

    #[test]
    fn tortilis_mega_grows_two_level_branches() {
        let mut world = flat_world();
        let mut tree = TreeVachelliaTortilisMega::default();
        tree.base.trunk_size = 10;
        tree.base.crown_size = 8;
        let mut rand = JavaRandom::new(33);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -48..64 {
            for z in -48..64 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        162 => logs += 1,
                        161 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // Mega 版有两级树枝，原木和树叶都应更多
        assert!(logs > 11, "Mega 伞刺金合欢原木太少：{logs}");
        assert!(leaves > 30, "Mega 伞刺金合欢树叶太少：{leaves}");
    }
}
