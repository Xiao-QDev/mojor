//! 对应 `rtg/tree/TreeRTGCeibaPentandra.php`：吉贝树（黑森林用）。
//!
//! 结构：从 `y+trunkSize` 到 `y+crownSize` 的短主干，`trunkSize>0` 时在
//! `y+trunkSize` 处发出 3 根"主干枝"（长度 trunkSize*1.7），顶部再发出
//! `branch` 根水平枝（长度 `length`），每根末端放一团球状树叶。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct TreeCeibaPentandra {
    pub base: TreeRTG,
    /// 对应 `$length`：水平树枝长度。
    pub length: f64,
    /// 对应 `$branch`：顶部水平树枝数量。
    pub branch: i32,
    /// 对应 `$verStart`：树枝初始垂直角。
    pub ver_start: f64,
    /// 对应 `$verRand`：树枝垂直角随机范围。
    pub ver_rand: f64,
}

impl Default for TreeCeibaPentandra {
    /// 对应 `constructTreeRTGCeibaPentandra0`：默认橡木。
    fn default() -> Self {
        let base = TreeRTG {
            log_block: block_rules::wood_state(0, false),
            leaves_block: block_rules::wood_state(0, true),
            trunk_size: 4,
            crown_size: 8,
            ..TreeRTG::default()
        };
        Self {
            base,
            length: f(5.0),
            branch: 3,
            ver_start: f(0.3199999928474426),
            ver_rand: f(0.10000000149011612),
        }
    }
}

impl Tree for TreeCeibaPentandra {
    fn furthest_likely_extension(&self) -> i32 {
        6
    }

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

        let trunk_log = self.base.get_trunk_log(log_block);

        // 主干：从 y+trunkSize 到 y+crownSize
        for i in (y + trunk_size)..(y + crown_size) {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, i, z));
        }

        // 主干枝：trunkSize>0 时在 y+trunkSize 处发 3 根
        if trunk_size > 0 {
            for k in 0..3 {
                let hor_dir = f(f(120.0 * k as f64) - f(40.0) + rand.next_int_bound(80) as f64);
                let ver_dir = f(f(1.600000023841858)
                    + f(rand.next_float() as f64 * f(0.10000000149011612)));
                let len = f(f(trunk_size as f64) * f(1.7000000476837158));
                self.generate_branch(
                    world, &mut tracker, rand, x as f64, (y + trunk_size) as f64, z as f64,
                    hor_dir, ver_dir, len, 1.0, trunk_log,
                );
            }
        }

        // 顶部水平枝 + 末端树叶团
        let branch_count = self.branch;
        let length = self.length;
        for j in 0..branch_count {
            let hor_dir = f(f(120.0 * j as f64) - f(60.0) + rand.next_int_bound(120) as f64);
            let ver_dir = f(self.ver_start + f(rand.next_float() as f64 * self.ver_rand));
            self.generate_branch(
                world, &mut tracker, rand, x as f64, (y + crown_size) as f64, z as f64,
                hor_dir, ver_dir, length, 1.0, log_block,
            );
            let ex = x + f(f((hor_dir * PI / 180.0).cos()) * ver_dir * length) as i32;
            let ez = z + f(f((hor_dir * PI / 180.0).sin()) * ver_dir * length) as i32;
            let ey = (y + crown_size) + f(f(1.0 - ver_dir) * length) as i32;
            self.generate_leaves(
                world, &mut tracker, rand, ex, ey, ez, f(4.0), f(1.2999999523162842),
                log_block, leaves_block, no_leaves,
            );
        }
        true
    }
}

impl TreeCeibaPentandra {
    /// 对应 `generateBranch`：从 (x,y,z) 沿 horDir/verDir 方向走 length 步放原木。
    #[allow(clippy::too_many_arguments)]
    fn generate_branch(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        _rand: &mut JavaRandom,
        x: f64,
        y: f64,
        z: f64,
        hor_dir: f64,
        ver_dir: f64,
        length: f64,
        speed: f64,
        log_block: i32,
    ) {
        let mut ver_dir = ver_dir;
        if ver_dir < 0.0 {
            ver_dir = f(-ver_dir);
        }
        let vel_y = f(1.0 - ver_dir);
        if ver_dir > 1.0 {
            ver_dir = f(1.0 - f(ver_dir - 1.0));
        }
        let vel_x = f(f((hor_dir * PI / 180.0).cos()) * ver_dir);
        let vel_z = f(f((hor_dir * PI / 180.0).sin()) * ver_dir);

        let mut px = f(x);
        let mut py = f(y);
        let mut pz = f(z);
        let mut c = f(0.0);
        while c < length {
            self.base.place_log_block(
                world,
                tracker,
                Pos::new(px as i32, py as i32, pz as i32),
                log_block,
            );
            px = f(px + vel_x);
            py = f(py + vel_y);
            pz = f(pz + vel_z);
            c = f(c + speed);
        }
    }

    /// 对应 `generateLeaves`：以 (x,y,z) 为中心放一团球状树叶。
    #[allow(clippy::too_many_arguments)]
    fn generate_leaves(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        size: f64,
        width: f64,
        log_block: i32,
        leaves_block: i32,
        no_leaves: bool,
    ) {
        let s = f(size - f(1.0)) as i32;
        let w = f(f(size - f(1.0)) * width) as i32;
        for i in -w..=w {
            for j in -s..=s {
                for k in -w..=w {
                    let dist = f(f(f(i as f64 / width).abs()) + f(j as f64).abs()
                        + f(f(k as f64 / width).abs()));
                    let inside = dist <= f(size - f(0.5))
                        || (dist <= size && rand.next_boolean());
                    if inside {
                        if dist < f(0.6000000238418579) {
                            self.base.place_log_block(
                                world,
                                tracker,
                                Pos::new(x + i, y + j, z + k),
                                log_block,
                            );
                        }
                        if !no_leaves {
                            self.base.place_leaves_block(
                                world,
                                tracker,
                                Pos::new(x + i, y + j, z + k),
                                leaves_block,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use std::collections::HashMap;

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
    fn ceiba_grows_trunk_branches_and_leaf_clusters() {
        let mut world = flat_world();
        let mut tree = TreeCeibaPentandra::default();
        tree.base.trunk_size = 4;
        tree.base.crown_size = 8;
        let mut rand = JavaRandom::new(7);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves) = (0, 0);
        for x in -32..48 {
            for z in -32..48 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        17 => logs += 1,
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        // 主干 4 根 + 3 根主干枝 + 3 根顶部枝 + 树叶团中心原木
        assert!(logs > 10, "吉贝树原木太少：{logs}");
        // 3 团球状树叶
        assert!(leaves > 20, "吉贝树树叶太少：{leaves}");
    }

    #[test]
    fn ceiba_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeCeibaPentandra::default();
        let mut rand = JavaRandom::new(7);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
