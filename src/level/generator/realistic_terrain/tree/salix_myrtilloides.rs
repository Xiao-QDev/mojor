//! 对应 `rtg/tree/TreeRTGSalixMyrtilloides.php`：沼泽柳树。
//!
//! 主干 13 格、6 根树枝每根长 6 格、顶部和树枝末端各一个树叶球。
//! PHP 里第二段"树枝中段树叶球"循环因为 `c` 未重置而永不进入，这里照抄。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;

#[derive(Clone, Debug)]
pub struct TreeSalixMyrtilloides {
    pub base: TreeRTG,
}

impl Default for TreeSalixMyrtilloides {
    fn default() -> Self {
        Self {
            base: TreeRTG::default(),
        }
    }
}

impl Tree for TreeSalixMyrtilloides {
    fn estimated_size(&self) -> f64 {
        let branch = f(6.0);
        f(f(branch * branch) / 30.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        19
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n3(world, pos, true) {
            return false;
        }
        let (x, y, z) = (pos.x, pos.y, pos.z);
        let trunk_log = self.base.get_trunk_log(self.base.log_block);
        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        let height = 13i32;
        let leave_height = 5i32;
        let branches = 6i32;
        let branch_length = 6i32;

        for i in 0..height {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
        }
        self.create_trunk(world, rand, x, y, z, &mut tracker, &trunk_log);
        self.create_leaves_around_branch(world, rand, x, y + height, z, 3, 2, &mut tracker);

        let angle_step = 360.0 / branches as f64;
        let mut dir = rand.next_int_bound(angle_step as i32) as f64;
        for _ in 0..branches {
            let mut c = 0.0f64;
            let mut hd = f((height as f64 - (rand.next_float() as f64 * leave_height as f64)) - 2.0);
            dir = f(dir + angle_step);
            let xd = f((dir * 3.141_592_653_589_793).to_radians().cos());
            let yd = f((dir * 3.141_592_653_589_793).to_radians().sin());
            let mut m = false;

            while c < f(branch_length as f64) {
                c += 1.0;
                hd = f(hd + 0.5);
                let bx = x + f(c * xd) as i32;
                let by = y + hd as i32;
                let bz = z + f(c * yd) as i32;
                self.base
                    .place_log_block(world, &mut tracker, Pos::new(bx, by, bz), trunk_log);
            }
            // PHP 的第二段循环 c 未重置，条件 `c < branchLength` 永不成立，照抄。
            while c < f(branch_length as f64) {
                if c > f((branch_length / 2) as f64) && !m {
                    m = true;
                    let bx = x + f(c * xd) as i32;
                    let by = y + hd as i32;
                    let bz = z + f(c * yd) as i32;
                    self.create_leaves_around_branch(world, rand, bx, by, bz, 2, 1, &mut tracker);
                }
                c += 1.0;
                hd = f(hd + 0.5);
            }
            let bx = x + f(c * xd) as i32;
            let by = y + hd as i32;
            let bz = z + f(c * yd) as i32;
            self.create_leaves_around_branch(world, rand, bx, by, bz, 2, 1, &mut tracker);
        }
        true
    }
}

impl TreeSalixMyrtilloides {
    /// 对应 `createTrunk`：基部五个方向各延伸一截木头，让树根更自然。
    fn create_trunk(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
        trunk_log: &i32,
    ) {
        let pos = [0, 0, 1, 0, 0, 1, -1, 0, 0, -1];
        for t in 0..5 {
            let mut sh = rand.next_int_bound(3) + y;
            while sh > y - 3 {
                let bx = x + pos[t * 2];
                let bz = z + pos[t * 2 + 1];
                self.base
                    .place_log_block(world, tracker, Pos::new(bx, sh, bz), *trunk_log);
                sh -= 1;
            }
        }
    }

    /// 对应 `createLeavesAroundBranch`：在 (x,y,z) 周围放一个半径 s 的树叶球。
    fn create_leaves_around_branch(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        s: i32,
        c: i32,
        tracker: &mut SkylightTracker,
    ) {
        if self.base.no_leaves {
            return;
        }
        let t = s * s;
        for i in -s..=s {
            for j in -s..=s {
                for k in -s..=s {
                    let l = i * i + j * j + k * k;
                    if l > t {
                        continue;
                    }
                    let edge = l < t - c;
                    if edge || rand.next_boolean() {
                        let placed = self.base.place_leaves_block(
                            world,
                            tracker,
                            Pos::new(x + i, y + j, z + k),
                            self.base.leaves_block,
                        );
                        if placed && j < -(s - 2) && rand.next_int_bound(3) != 0 {
                            let distance = (i.abs() + k.abs() + j.abs()) as i32;
                            self.create_vine(world, rand, x + i, y + j, z + k, 6 - distance, tracker);
                        }
                    }
                }
            }
        }
    }

    /// 对应 `createVine`：从 (x,y,z) 往下挂几格树叶当藤蔓。
    fn create_vine(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        max_length: i32,
        tracker: &mut SkylightTracker,
    ) {
        let r = max_length - rand.next_int_bound(3);
        let mut i = -1;
        while i > -r {
            self.base.place_leaves_block(
                world,
                tracker,
                Pos::new(x, y + i, z),
                self.base.leaves_block,
            );
            i -= 1;
        }
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
    fn willow_grows_trunk_branches_and_leaves() {
        let mut world = flat_world();
        let mut tree = TreeSalixMyrtilloides::default();
        let mut rand = JavaRandom::new(7);
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
        // 主干 13 格 + 6 根树枝各 6 格 + 树根延伸，原木应远多于 13
        assert!(logs > 13, "柳树原木太少：{logs}");
        // 顶部树叶球半径 3，六根树枝末端各一个半径 2 的球
        assert!(leaves > 50, "柳树树叶太少：{leaves}");
    }

    #[test]
    fn willow_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeSalixMyrtilloides::default();
        let mut rand = JavaRandom::new(7);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
