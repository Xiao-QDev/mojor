//! 对应 `rtg/tree/TreeRTGCocosNucifera.php`：椰子树（丛林/沙滩用）。
//!
//! 结构：从地面沿一个随机水平方向（N/S/E/W）倾斜生长的弯曲树干，
//! 顶端放一簇预计算好的树叶偏移 + 偶尔结可可豆。整棵树用 `posX/Y/Z`
//! 浮点坐标累加，每一步用 `f()` 截断到 float32，落点再 `as i32` 向零截断。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::world::GenWorld;
use std::f64::consts::PI;

/// 对应 PHP `$leaves`：45 组 (x, y, z) 树叶偏移，相对树干顶端。
const LEAVES: &[(i32, i32, i32)] = &[
    (1, 0, 0), (2, 0, 0), (3, -1, 0), (3, -2, 0),
    (-1, 0, 0), (-2, 0, 0), (-3, -1, 0), (-3, -2, 0),
    (0, 0, 1), (0, 0, 2), (0, -1, 3), (0, -2, 3),
    (0, 0, -1), (0, 0, -2), (0, -1, -3), (0, -2, -3),
    (0, 1, 0), (1, 1, 1), (-1, 1, -1), (-1, 1, 1), (1, 1, -1),
    (1, 2, 0), (-1, 2, 0), (0, 2, 1), (0, 2, -1),
    (2, 3, 0), (3, 3, 0), (4, 2, 0), (-2, 3, 0), (-3, 3, 0), (-4, 2, 0),
    (0, 3, 2), (0, 3, 3), (0, 2, 4), (0, 3, -2), (0, 3, -3), (0, 2, -4),
    (2, 2, -2), (-2, 2, 2), (-2, 2, -2), (2, 2, 2),
    (3, 2, -3), (-3, 2, 3), (-3, 2, -3), (3, 2, 3),
];

/// 对应 PHP `$cocoas`：4 组 (x, y, z) 可可豆偏移，每颗 1/20 概率出现。
const COCOAS: &[(i32, i32, i32)] = &[
    (0, -2, 1), (1, -2, 0), (0, -2, -1), (-1, -2, 0),
];

#[derive(Clone, Debug)]
pub struct TreeCocosNucifera {
    pub base: TreeRTG,
}

impl Default for TreeCocosNucifera {
    /// 对应 `constructTreeRTGCocosNucifera0`：默认丛林木材。
    fn default() -> Self {
        let base = TreeRTG {
            log_block: block_rules::wood_state(3, false),
            leaves_block: block_rules::wood_state(3, true),
            trunk_size: 8,
            crown_size: 7,
            ..TreeRTG::default()
        };
        Self { base }
    }
}

impl Tree for TreeCocosNucifera {
    fn furthest_likely_extension(&self) -> i32 {
        4
    }

    fn estimated_size(&self) -> f64 {
        let ext = self.furthest_likely_extension() as f64;
        f(f(ext * ext) / 16.0)
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        if !self.base.is_ground_valid_n3(world, pos, true) {
            return false;
        }
        let (x, y, z) = (pos.x as f64, pos.y as f64, pos.z as f64);
        let trunk_size = self.base.trunk_size;
        let crown_size = self.base.crown_size;
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

        // 树皮覆盖的原木
        let trunk_log = self.base.get_trunk_log(log_block);

        // 水平方向：N/S/E/W 四选一
        let hor_dir = get_random_dir(rand) as f64;
        let mut ver_dir = f(0.30000001192092896 + f(rand.next_float() as f64 * 0.4000000059604645));
        if ver_dir < 0.0 {
            ver_dir = f(-ver_dir);
        }
        let mut vel_y = f(1.0 - ver_dir);
        if ver_dir > 1.0 {
            ver_dir = f(1.0 - f(ver_dir - 1.0));
        }
        let mut vel_x = f(f((hor_dir * PI / 180.0).cos()) * ver_dir);
        let mut vel_z = f(f((hor_dir * PI / 180.0).sin()) * ver_dir);

        let length = f((trunk_size + crown_size) as f64);
        let mut pos_x = f(x);
        let mut pos_y = f(y);
        let mut pos_z = f(z);
        let mut c = f(0.0);

        while c < length {
            self.base.place_log_block(
                world,
                &mut tracker,
                Pos::new(pos_x as i32, pos_y as i32, pos_z as i32),
                trunk_log,
            );
            if c < f(length - f(3.0)) {
                let loss = f(vel_x.abs() + vel_z.abs());
                vel_x = f(vel_x * 0.8999999761581421);
                pos_x = f(pos_x + vel_x);
                vel_z = f(vel_z * 0.8999999761581421);
                pos_z = f(pos_z + vel_z);
                let loss = f(loss - f(vel_x.abs() + vel_z.abs()));
                vel_y = f(vel_y + loss);
                pos_y = f(pos_y + vel_y);
            } else {
                pos_y = f(pos_y + vel_y);
            }
            c = f(c + 1.0);
        }

        let lx = pos_x as i32;
        let ly = pos_y as i32 - 1;
        let lz = pos_z as i32;

        if !no_leaves {
            for &(dx, dy, dz) in LEAVES {
                self.base.place_leaves_block(
                    world,
                    &mut tracker,
                    Pos::new(lx + dx, ly + dy, lz + dz),
                    leaves_block,
                );
            }
        }
        // 可可豆：block 2032 = (127 << 4)
        for &(dx, dy, dz) in COCOAS {
            if rand.next_int_bound(20) == 0 {
                world.set_block_state(lx + dx, ly + dy, lz + dz, 2032);
            }
        }
        let _ = generate_flag;
        true
    }
}

/// 对应 `getRandomDir`：-180 / -90 / 0 / 90 四选一。
fn get_random_dir(rand: &mut JavaRandom) -> i32 {
    match rand.next_int_bound(4) {
        0 => -180,
        1 => -90,
        2 => 0,
        _ => 90,
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
    fn palm_grows_trunk_and_leaves() {
        let mut world = flat_world();
        let mut tree = TreeCocosNucifera::default();
        tree.base.trunk_size = 8;
        tree.base.crown_size = 7;
        let mut rand = JavaRandom::new(3);
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
        // 弯曲树干至少 8 根原木
        assert!(logs >= 8, "椰子树原木太少：{logs}");
        // 45 组树叶偏移
        assert!(leaves > 20, "椰子树树叶太少：{leaves}");
    }

    #[test]
    fn palm_refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeCocosNucifera::default();
        let mut rand = JavaRandom::new(3);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }
}
