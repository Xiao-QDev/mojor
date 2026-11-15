//! 对应 `rtg/tree/TreeRTGPiceaSitchensis.php`：中型云杉。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreePiceaSitchensis {
    pub base: TreeRTG,
}

impl Default for TreePiceaSitchensis {
    fn default() -> Self {
        let materials = TreeMaterials::SPRUCE;
        Self {
            base: TreeRTG {
                log_block: materials.log,
                leaves_block: materials.leaves,
                branch_block: materials.branches,
                trunk_size: 8,
                crown_size: 10,
                ..TreeRTG::default()
            },
        }
    }
}

impl Tree for TreePiceaSitchensis {
    fn estimated_size(&self) -> f64 {
        let branch_length = f((self.base.crown_size / 4 + 1) as f64);
        f(f(branch_length * branch_length) / 30.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        2 + self.base.crown_size / 4 + 1
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
        for _ in 0..self.base.trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y, z));
            y += 1;
        }

        let (mut previous_x, mut previous_z) = (0, 0);
        let mut i = 0;
        while i < self.base.crown_size {
            // nextInt(2) 在高度判断之前， RNG 必须每层都消耗。
            if rand.next_int_bound(2) <= 0 && i < self.base.crown_size - 2 {
                let mut dx = -1 + rand.next_int_bound(3);
                let mut dz = -1 + rand.next_int_bound(3);
                if dx == 0 && dz == 0 {
                    dx = -1 + rand.next_int_bound(3);
                    dz = -1 + rand.next_int_bound(3);
                }
                if previous_x != 0 && previous_x == dx && rand.next_int_bound(3) < 2 {
                    dx = -dx;
                }
                if previous_z != 0 && previous_z == dz && rand.next_int_bound(3) < 2 {
                    dz = -dz;
                }
                previous_x = dx;
                previous_z = dz;
                let leaf_size = if i < self.base.crown_size - 3 { 2 } else { 1 };
                let branch_size = ((self.base.crown_size - i - 3) / 3).max(0);
                self.build_branch(
                    world,
                    rand,
                    x,
                    y,
                    z,
                    dx,
                    dz,
                    branch_size,
                    leaf_size,
                    &mut tracker,
                );
                dx = -dx;
                dz = -dz;
                if dx == 0 || dz == 0 {
                    if dx == 0 {
                        self.build_branch(
                            world,
                            rand,
                            x,
                            y,
                            z,
                            1,
                            dz,
                            branch_size,
                            leaf_size,
                            &mut tracker,
                        );
                        self.build_branch(
                            world,
                            rand,
                            x,
                            y,
                            z,
                            -1,
                            dz,
                            branch_size,
                            leaf_size,
                            &mut tracker,
                        );
                    } else {
                        self.build_branch(
                            world,
                            rand,
                            x,
                            y,
                            z,
                            dx,
                            -1,
                            branch_size,
                            leaf_size,
                            &mut tracker,
                        );
                        self.build_branch(
                            world,
                            rand,
                            x,
                            y,
                            z,
                            dx,
                            1,
                            branch_size,
                            leaf_size,
                            &mut tracker,
                        );
                    }
                } else {
                    self.build_branch(
                        world,
                        rand,
                        x,
                        y,
                        z,
                        0,
                        dz,
                        branch_size,
                        leaf_size,
                        &mut tracker,
                    );
                    self.build_branch(
                        world,
                        rand,
                        x,
                        y,
                        z,
                        dx,
                        0,
                        branch_size,
                        leaf_size,
                        &mut tracker,
                    );
                }
                i += 1;
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
            if i < self.base.crown_size - 2 {
                for (dx, dz) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                    if rand.next_boolean() {
                        self.build_leaves(world, x + dx, y, z + dz, &mut tracker);
                    }
                }
            }
            y += 1;
            i += 1;
        }
        self.build_leaves(world, x, y - 1, z + 1, &mut tracker);
        self.build_leaves(world, x, y - 1, z - 1, &mut tracker);
        self.build_leaves(world, x + 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x - 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x, y, z, &mut tracker);
        true
    }
}

impl TreePiceaSitchensis {
    #[allow(clippy::too_many_arguments)]
    fn build_branch(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        dx: i32,
        dz: i32,
        log_length: i32,
        leaf_size: i32,
        tracker: &mut SkylightTracker,
    ) {
        for i in -2_i32..=2 {
            for j in -2_i32..=2 {
                let ragged = i32::from(rand.next_int_bound(4) == 0);
                if i.abs() + j.abs() + ragged < leaf_size + 1 {
                    self.build_leaves(
                        world,
                        x + i + dx * log_length,
                        y,
                        z + j + dz * log_length,
                        tracker,
                    );
                }
            }
        }
        self.build_leaves(
            world,
            x + dx * log_length,
            y + 1,
            z + dz * log_length,
            tracker,
        );
        for m in 1..=log_length {
            self.base.place_log_block(
                world,
                tracker,
                Pos::new(x + dx * m, y, z + dz * m),
                self.base.branch_block,
            );
            if m > 1 {
                for i in -1_i32..=1 {
                    for j in -1_i32..=1 {
                        let ragged = i32::from(rand.next_int_bound(4) == 0);
                        if i.abs() + j.abs() + ragged < leaf_size + 1 {
                            self.build_leaves(world, x + i + dx * m, y, z + j + dz * m, tracker);
                        }
                    }
                }
            }
        }
    }

    fn build_leaves(
        &self,
        world: &mut GenWorld,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
    ) {
        if !self.base.no_leaves {
            self.base
                .place_leaves_block(world, tracker, Pos::new(x, y, z), self.base.leaves_block);
        }
    }
}
