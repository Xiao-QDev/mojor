//! 对应 `rtg/tree/TreeRTGBetulaUtilis.php`：小型白桦。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreeBetulaUtilis {
    pub base: TreeRTG,
}

impl Default for TreeBetulaUtilis {
    fn default() -> Self {
        Self {
            base: TreeRTG::default(),
        }
    }
}

impl Tree for TreeBetulaUtilis {
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

        // PHP 是 ceil(intdiv(crownSize, 4))：intdiv 已经向零取整，ceil 不再改变结果。
        let small = self.base.crown_size / 4;
        let large = self.base.crown_size - small;
        for _ in 0..2 {
            if !self.base.no_leaves {
                for dx in -1_i32..=1 {
                    for dz in -1_i32..=1 {
                        if dx.abs() + dz.abs() < 2 {
                            self.base.place_leaves_block(
                                world,
                                &mut tracker,
                                Pos::new(x + dx, y, z + dz),
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

        for i in 0..small - 3 {
            if !self.base.no_leaves {
                for dx in -1_i32..=1 {
                    for dz in -1_i32..=1 {
                        // 保持 PHP 的短路：十字格不消耗 nextInt。
                        if dx.abs() + dz.abs() < 2 || rand.next_int_bound(small) < i {
                            self.base.place_leaves_block(
                                world,
                                &mut tracker,
                                Pos::new(x + dx, y, z + dz),
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

        for i in 0..large {
            if !self.base.no_leaves {
                let cap = i.max(large - 3 * (large - i));
                for dx in -2_i32..=2 {
                    for dz in -2_i32..=2 {
                        let interior = dx > -2 && dz > -2 && dx < 2 && dz < 2;
                        if dx.abs() + dz.abs() != 4
                            && (interior || rand.next_int_bound(large) < cap)
                        {
                            self.base.place_leaves_block(
                                world,
                                &mut tracker,
                                Pos::new(x + dx, y, z + dz),
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

        if !self.base.no_leaves {
            for dx in -1_i32..=1 {
                for dz in -1_i32..=1 {
                    if dx.abs() + dz.abs() < 2 || rand.next_int_bound(4) != 0 {
                        self.base.place_leaves_block(
                            world,
                            &mut tracker,
                            Pos::new(x + dx, y, z + dz),
                            self.base.leaves_block,
                        );
                    }
                }
            }
        }
        self.base.place_leaves_block(
            world,
            &mut tracker,
            Pos::new(x, y, z),
            self.base.leaves_block,
        );
        y += 1;
        self.base.place_leaves_block(
            world,
            &mut tracker,
            Pos::new(x, y, z),
            self.base.leaves_block,
        );

        if !self.base.no_leaves {
            for target in [
                Pos::new(x + 1, y, z),
                Pos::new(x - 1, y, z),
                Pos::new(x, y, z + 1),
                Pos::new(x, y, z - 1),
            ] {
                if rand.next_int_bound(2) != 0 {
                    self.base.place_leaves_block(
                        world,
                        &mut tracker,
                        target,
                        self.base.leaves_block,
                    );
                }
            }
        }
        true
    }
}
