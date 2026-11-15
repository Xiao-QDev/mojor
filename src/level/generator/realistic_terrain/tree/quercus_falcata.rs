//! 对应 `rtg/tree/TreeRTGQuercusFalcata.php`：中型橡树。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::branch::TreeBranch;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreeQuercusFalcata {
    pub base: TreeRTG,
    pub interbranch: f64,
    pub additional_branch_length: f64,
}

impl Default for TreeQuercusFalcata {
    fn default() -> Self {
        Self {
            base: TreeRTG {
                trunk_reserve: 2,
                ..TreeRTG::default()
            },
            interbranch: f(0.4),
            additional_branch_length: 1.5,
        }
    }
}

impl Tree for TreeQuercusFalcata {
    fn estimated_size(&self) -> f64 {
        let branch_length =
            f(f(f(f(self.base.crown_size as f64) / 4.0) + self.additional_branch_length) + 1.0);
        f(f(branch_length * branch_length) / 20.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        let branch_length =
            f(f(f(f(self.base.crown_size as f64) / 4.0) + self.additional_branch_length) + 4.5);
        branch_length.ceil() as i32
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn set_crown_size(&mut self, size: i32) {
        self.base.crown_size = size - 2;
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
        }
        for (root_x, root_z) in [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)] {
            self.build_trunk(world, rand, root_x, y, root_z, &mut tracker);
        }
        for crown in 0..self.base.crown_size {
            self.base.place_log_block(
                world,
                &mut tracker,
                pos.up_n(crown + self.base.trunk_size - 1),
                self.base.log_block,
            );
        }
        self.place_leaves_blob(
            world,
            pos.up_n(self.base.crown_size + self.base.trunk_size - 1),
            &mut tracker,
        );

        let mut height = f(self.base.crown_size as f64);
        let mut horizontal = rand.next_float() as f64 * std::f64::consts::PI * 2.0;
        while height > 0.0 {
            let ascent = f(
                f(f(f(1.5 * height) / f(self.base.crown_size as f64)) - f(0.2))
                    + f(rand.next_float() as f64 * 0.4),
            );
            let mut branch_length = f(f(f(self.base.crown_size as f64) - height) / 4.0);
            branch_length = f(branch_length + self.additional_branch_length);
            branch_length = f(f(branch_length - 0.5) + rand.next_float() as f64);
            if branch_length > 2.0 {
                let start = pos.up_n(self.base.trunk_size + height as i32 - 1);
                if branch_length < 3.0 {
                    let branch = TreeBranch::new(horizontal, ascent, branch_length, 0, start);
                    self.make_log_branch(world, rand, &branch, &mut tracker);
                } else {
                    let branch =
                        TreeBranch::new(horizontal, ascent, f(branch_length * 0.7), 0, start);
                    self.make_bifurcating_branch(world, rand, &branch, &mut tracker);
                }
            }
            horizontal += f(1.4 + rand.next_float() as f64);
            height = f(height - f(f(self.interbranch) * f(1.0 + rand.next_float() as f64)));
        }
        true
    }
}

impl TreeQuercusFalcata {
    fn build_trunk(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
    ) {
        let mut height = (f(self.base.trunk_size as f64) / 4.0).floor() as i32;
        height = height - 3 + rand.next_int_bound(4);
        if height <= 0 {
            return;
        }
        for step in -1..height {
            self.base
                .place_trunk_block(world, tracker, Pos::new(x, y + step, z));
        }
    }

    fn make_log_branch(
        &self,
        world: &mut GenWorld,
        _rand: &mut JavaRandom,
        branch: &TreeBranch,
        tracker: &mut SkylightTracker,
    ) {
        let mut branch = branch.clone();
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
        }
        self.place_leaves_blob(world, branch.location(), tracker);
    }

    fn make_bifurcating_branch(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        branch: &TreeBranch,
        tracker: &mut SkylightTracker,
    ) {
        let mut branch = branch.clone();
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
        }
        for offset in [0.5, -1.0] {
            let direction = branch.initial_horizontal + offset + f(rand.next_float() as f64 * 0.5);
            let ascent = f(f(branch.initial_vertical - 0.2) + f(rand.next_float() as f64 * 0.4));
            let length = f(branch.initial_length * f(0.6 + f(rand.next_float() as f64 * 0.2)));
            let child = TreeBranch::new(
                direction,
                ascent,
                length,
                branch.stage + 1,
                branch.location(),
            );
            self.make_log_branch(world, rand, &child, tracker);
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

    fn place_leaves_blob(&self, world: &mut GenWorld, center: Pos, tracker: &mut SkylightTracker) {
        for dy in 0..2 {
            for dz in -2..3 {
                for dx in -2..3 {
                    if dz * dz + dx * dx == 8 {
                        continue;
                    }
                    self.place_leaves(world, tracker, center.add(dx, dy, dz));
                }
            }
        }
        let top = center.up_n(2);
        for target in [top, top.east(), top.west(), top.north(), top.south()] {
            self.place_leaves(world, tracker, target);
        }
    }

    fn place_leaves(&self, world: &mut GenWorld, tracker: &mut SkylightTracker, pos: Pos) {
        self.base
            .place_leaves_block(world, tracker, pos, self.base.leaves_block);
    }
}
