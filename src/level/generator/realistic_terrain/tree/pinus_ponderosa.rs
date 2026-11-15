//! 对应 `rtg/tree/TreeRTGPinusPonderosa.php`：大型黄松。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreePinusPonderosa {
    pub base: TreeRTG,
}

impl Default for TreePinusPonderosa {
    fn default() -> Self {
        Self {
            base: TreeRTG::default(),
        }
    }
}

impl Tree for TreePinusPonderosa {
    fn estimated_size(&self) -> f64 {
        let branch_length = f((self.base.crown_size / 6 + 1) as f64);
        f(f(branch_length * branch_length) / 30.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        9
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        let (x, mut y, z) = (pos.x, pos.y, pos.z);
        let trunk_log = self.base.get_trunk_log(self.base.log_block);
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
            if !self
                .base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y, z))
            {
                return false;
            }
            if i > 5 && rand.next_int_bound(7) == 0 {
                let mut dx = -1 + rand.next_int_bound(3);
                let mut dz = -1 + rand.next_int_bound(3);
                if dx == 0 && dz == 0 {
                    dx = -1 + rand.next_int_bound(3);
                    dz = -1 + rand.next_int_bound(3);
                }
                self.build_branch(world, x, y, z, dx, dz, 1, 1, &mut tracker);
            }
            y += 1;
        }

        y = pos.y;
        for (root_x, root_z) in [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)] {
            self.build_trunk(world, rand, root_x, y, root_z, trunk_log, &mut tracker);
        }
        y += self.base.trunk_size;
        let (mut previous_x, mut previous_z) = (0, 0);
        for i in 0..self.base.crown_size {
            let bound = if i < self.base.crown_size - 12 && i > 2 {
                3
            } else {
                2
            };
            if rand.next_int_bound(bound) <= 1 && i < self.base.crown_size - 2 {
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
                let log_size = if i < self.base.crown_size - 12 && i > 3 {
                    3
                } else if i < self.base.crown_size - 8 {
                    2
                } else {
                    1
                };
                let leaf_size = if i < self.base.crown_size - 4 { 2 } else { 1 };
                self.build_branch(world, x, y, z, dx, dz, log_size, leaf_size, &mut tracker);
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
        }
        self.build_leaves(world, x, y - 1, z + 1, &mut tracker);
        self.build_leaves(world, x, y - 1, z - 1, &mut tracker);
        self.build_leaves(world, x + 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x - 1, y - 1, z, &mut tracker);
        self.build_leaves(world, x, y, z, &mut tracker);
        true
    }
}

impl TreePinusPonderosa {
    #[allow(clippy::too_many_arguments)]
    fn build_trunk(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        trunk_log: i32,
        tracker: &mut SkylightTracker,
    ) {
        let mut height = (f(self.base.trunk_size as f64) / 4.0).ceil() as i32;
        height += rand.next_int_bound(height * 2);
        for step in -1..height {
            self.base
                .place_log_block(world, tracker, Pos::new(x, y + step, z), trunk_log);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_branch(
        &self,
        world: &mut GenWorld,
        x: i32,
        y: i32,
        z: i32,
        dx: i32,
        dz: i32,
        log_length: i32,
        leaf_size: i32,
        tracker: &mut SkylightTracker,
    ) {
        for i in -1_i32..=1 {
            for j in -1_i32..=1 {
                if i.abs() + j.abs() < leaf_size + 1 {
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
            let target = Pos::new(x + dx * m, y, z + dz * m);
            if m < log_length {
                self.base
                    .place_log_block(world, tracker, target, self.base.log_block);
            } else {
                // 参考实现这里传的确实是 logBlock，而不是 leavesBlock。
                self.base
                    .place_leaves_block(world, tracker, target, self.base.log_block);
            }
            if m > 1 {
                for i in -1_i32..=1 {
                    for j in -1_i32..=1 {
                        if i.abs() + j.abs() < leaf_size + 1 {
                            self.build_leaves(world, x + i + dx * m, y, z + j + dz * m, tracker);
                        }
                    }
                }
                self.build_leaves(world, x + dx * m, y + 1, z + dz * m, tracker);
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
