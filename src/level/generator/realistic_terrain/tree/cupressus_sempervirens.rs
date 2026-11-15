//! 对应 `rtg/tree/TreeRTGCupressusSempervirens.php`：小型柏木。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreeCupressusSempervirens {
    pub base: TreeRTG,
}

impl Default for TreeCupressusSempervirens {
    fn default() -> Self {
        Self {
            base: TreeRTG::default(),
        }
    }
}

impl Tree for TreeCupressusSempervirens {
    fn estimated_size(&self) -> f64 {
        f(f(3.0 * 3.0) / 16.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        3
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

        // PHP 是 ceil(intdiv(crownSize, 2))：保留 intdiv 的向零取整语义。
        let small = self.base.crown_size / 2;
        let large = small;
        for _ in 0..large {
            if !self.base.no_leaves {
                for dx in -2_i32..=2 {
                    for dz in -2_i32..=2 {
                        let interior = dx > -2 && dz > -2 && dx < 2 && dz < 2;
                        if dx.abs() + dz.abs() != 4 && (interior || rand.next_int_bound(4) != 0) {
                            self.place_leaves(world, &mut tracker, x + dx, y, z + dz);
                        }
                    }
                }
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
            y += 1;
        }

        for i in 0..small {
            if !self.base.no_leaves {
                for dx in -1_i32..=1 {
                    for dz in -1_i32..=1 {
                        if dx.abs() + dz.abs() < 2 || rand.next_int_bound(4) != 0 {
                            self.place_leaves(world, &mut tracker, x + dx, y, z + dz);
                        }
                    }
                }
                if i == 0 {
                    for (dx, dz) in [
                        (1, 0),
                        (-1, 0),
                        (0, 1),
                        (0, -1),
                        (2, 0),
                        (-2, 0),
                        (0, 2),
                        (0, -2),
                    ] {
                        self.place_leaves(world, &mut tracker, x + dx, y, z + dz);
                    }
                }
            }
            self.base
                .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
            y += 1;
        }

        self.base
            .place_log_block(world, &mut tracker, Pos::new(x, y, z), self.base.log_block);
        if !self.base.no_leaves {
            for (dx, dy, dz) in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 0, 1),
                (0, 0, -1),
                (0, 1, 0),
                (0, 2, 0),
            ] {
                self.place_leaves(world, &mut tracker, x + dx, y + dy, z + dz);
            }
        }
        true
    }
}

impl TreeCupressusSempervirens {
    fn place_leaves(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        x: i32,
        y: i32,
        z: i32,
    ) {
        if !self.base.no_leaves {
            self.base
                .place_leaves_block(world, tracker, Pos::new(x, y, z), self.base.leaves_block);
        }
    }
}
