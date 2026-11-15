//! 对应 `rtg/tree/TreeRTGBetulaNigra.php`：中型白桦。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::branch::TreeBranch;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreeBetulaNigra {
    pub base: TreeRTG,
    pub branch_lengthening: f64,
    pub shortest_branch: f64,
    pub lowest_interbranch: f64,
    pub interbranch_lengthing: f64,
    pub leafy_log_frequency: f64,
}

impl Default for TreeBetulaNigra {
    fn default() -> Self {
        let materials = TreeMaterials::BIRCH;
        Self {
            base: TreeRTG {
                crown_size: 20,
                trunk_size: 5,
                branch_block: materials.branches,
                leaves_block: materials.leaves,
                log_block: materials.log,
                trunk_proportion_variability: f(0.2),
                lowest_variable_trunk_proportion: f(0.2),
                ..TreeRTG::default()
            },
            branch_lengthening: f(0.15),
            shortest_branch: f(1.6),
            lowest_interbranch: f(0.3),
            interbranch_lengthing: f(0.01),
            leafy_log_frequency: f(0.2),
        }
    }
}

impl Tree for TreeBetulaNigra {
    fn estimated_size(&self) -> f64 {
        let branch_length = f(self.shortest_branch
            + f(f(self.branch_lengthening * f(self.base.crown_size as f64)) / 3.0));
        f(f(branch_length * branch_length) / 15.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        let branch_length = f(self.shortest_branch
            + f(f(self.branch_lengthening * f(self.base.crown_size as f64)) / 1.5));
        branch_length.ceil() as i32
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        let (x, y, z) = (pos.x, pos.y, pos.z);
        let ground = [pos.east(), pos.west(), pos.south(), pos.north()];
        if !self.base.is_ground_valid_list(world, &ground) {
            return false;
        }
        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );
        for i in 0..self.base.trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
            if i > 5 && rand.next_int_bound(7) == 0 {
                let mut dx = -1 + rand.next_int_bound(3);
                let mut dz = -1 + rand.next_int_bound(3);
                if dx == 0 && dz == 0 {
                    dx = -1 + rand.next_int_bound(3);
                    dz = -1 + rand.next_int_bound(3);
                }
                let _ = (dx, dz);
            }
        }
        for crown in 0..self.base.crown_size {
            if !self.base.place_trunk_block(
                world,
                &mut tracker,
                pos.up_n(crown + self.base.trunk_size),
            ) {
                return false;
            }
        }
        for (root_x, root_z) in [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)] {
            self.base
                .build_trunk(world, rand, root_x, y, root_z, &mut tracker);
        }
        let top = pos.up_n(self.base.crown_size + self.base.trunk_size);
        for target in [
            top,
            top.up(),
            top.east(),
            top.west(),
            top.north(),
            top.south(),
        ] {
            self.place_leaves(world, &mut tracker, target);
        }
        let skirting = top.up();
        for target in [
            skirting.up(),
            skirting.east(),
            skirting.west(),
            skirting.north(),
            skirting.south(),
        ] {
            self.place_leaves(world, &mut tracker, target);
        }

        let mut height = f(f(self.base.crown_size as f64) - 1.0);
        let mut horizontal = rand.next_float() as f64 * std::f64::consts::PI * 2.0;
        let mut placed_branches = 0;
        while height > 0.0 {
            let ascent = f(f(1.0 + f(f(height / f(self.base.crown_size as f64)) * 0.2))
                + f(rand.next_float() as f64 * 0.2));
            let from_top = f(self.shortest_branch
                + f(
                    f(self.branch_lengthening * f(f(self.base.crown_size as f64) - height)) * 1.25,
                ));
            let from_bottom =
                f(self.shortest_branch + f(f(self.branch_lengthening * height) * 1.75));
            let mut length = from_top.min(from_bottom);
            let start = pos.up_n(self.base.trunk_size + height as i32 - 1);
            if length < 3.0 {
                let branch = TreeBranch::new(horizontal, ascent, length, 2, start);
                self.make_leaf_branch(world, &branch, &mut tracker);
            } else if length < 5.0 {
                length = f(length - 2.0);
                let branch = TreeBranch::new(horizontal, ascent, length, 2, start);
                self.make_leafy_log_branch(world, rand, &branch, &mut tracker);
            } else {
                length = f(length - 2.0);
                length = f(length * 0.7);
                let branch = TreeBranch::new(horizontal, ascent, length, 1, start);
                self.make_leafy_log_branch(world, rand, &branch, &mut tracker);
            }
            horizontal += f(1.5 + rand.next_float() as f64);
            if horizontal > std::f64::consts::PI * 2.0 {
                horizontal -= std::f64::consts::PI * 2.0;
            }
            let drop =
                f(self.lowest_interbranch
                    + f(self.interbranch_lengthing * f(placed_branches as f64)));
            height = f(height - f(drop * f(1.0 + rand.next_float() as f64)));
            placed_branches += 1;
        }
        true
    }
}

impl TreeBetulaNigra {
    fn make_leafy_log_branch(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        branch: &TreeBranch,
        tracker: &mut SkylightTracker,
    ) {
        let mut branch = branch.clone();
        let mut leaf_connections = TreeBranch::copied(&branch);
        while branch.not_done() {
            let old = branch.location();
            let location = branch.moved();
            if !self
                .base
                .place_log_block(world, tracker, location, self.base.branch_block)
            {
                return;
            }
            self.place_branch_bridges(world, tracker, old, location);
            self.place_leaves(world, tracker, branch.location().up());
            let frequency = f(self.leafy_log_frequency * f(branch.stage as f64));
            for target in [
                location.up(),
                location.east(),
                location.west(),
                location.north(),
                location.south(),
            ] {
                if (rand.next_float() as f64) < frequency {
                    self.place_leaves(world, tracker, target);
                }
            }
        }
        while leaf_connections.not_done() {
            let target = leaf_connections.moved_orthogonally();
            self.place_leaves(world, tracker, target);
        }
        if branch.stage == 1 {
            for offset in [0.5, -0.8] {
                let direction =
                    branch.initial_horizontal + offset + f(rand.next_float() as f64 * 0.3);
                let ascent =
                    f(f(branch.initial_vertical - 0.4) + f(rand.next_float() as f64 * 0.8));
                let length = f(branch.initial_length * f(0.5 + f(rand.next_float() as f64 * 0.2)));
                let child = TreeBranch::new(
                    direction,
                    ascent,
                    length,
                    branch.stage + 1,
                    branch.location(),
                );
                self.make_leafy_log_branch(world, rand, &child, tracker);
            }
        }
        if branch.stage == 2 {
            for offset in [0.5, -1.0] {
                let direction =
                    branch.initial_horizontal + offset + f(rand.next_float() as f64 * 0.5);
                let ascent =
                    f(f(branch.initial_vertical - 0.4) + f(rand.next_float() as f64 * 0.4));
                let length = f(2.0 * f(0.9 + f(rand.next_float() as f64 * 0.2)));
                let child = TreeBranch::new(
                    direction,
                    ascent,
                    length,
                    branch.stage + 1,
                    branch.location(),
                );
                self.make_leaf_branch(world, &child, tracker);
            }
        }
    }

    fn make_leaf_branch(
        &self,
        world: &mut GenWorld,
        branch: &TreeBranch,
        tracker: &mut SkylightTracker,
    ) {
        let mut branch = branch.clone();
        while branch.not_done() {
            let target = branch.moved();
            if !self.place_leaves(world, tracker, target) {
                return;
            }
            for adjacent in [
                target.up(),
                target.east(),
                target.west(),
                target.north(),
                target.south(),
            ] {
                self.place_leaves(world, tracker, adjacent);
            }
        }
    }

    fn place_branch_bridges(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        old: Pos,
        location: Pos,
    ) {
        if old.y != location.y {
            self.place_leaves(world, tracker, Pos::new(location.x, old.y, location.z));
            if old.x != location.x {
                self.place_leaves(world, tracker, Pos::new(old.x, old.y, location.z));
            }
            if old.z != location.z {
                self.place_leaves(world, tracker, Pos::new(location.x, old.y, old.z));
            }
        }
    }

    fn place_leaves(&self, world: &mut GenWorld, tracker: &mut SkylightTracker, pos: Pos) -> bool {
        self.base
            .place_leaves_block(world, tracker, pos, self.base.leaves_block)
    }
}
