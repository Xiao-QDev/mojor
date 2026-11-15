//! 对应 `rtg/tree/TreeRTGQuercusRobur.php`：RTG 的橡木（"大橡树"风格）。
//!
//! 和 vanilla 的"一根树干 + 几团叶子"不同，这棵是 **BigTree** 血统：
//!
//! 1. `generate_leaf_node_list` 先沿主干按高度分层，用 `layer_size` 算出每层的
//!    圆盘半径，再在圆盘里随机撒一圈"叶团"坐标
//! 2. `generate_trunk` 从地面往上一格一格铺原木（树干可以 1×1 或 2×2）
//! 3. `generate_leaf_node_bases` 把每个叶团从树干用一条斜线连过去——所以橡树
//!    有那种粗壮的斜枝
//! 4. `generate_leaves` 在每个叶团位置画一摞圆盘
//!
//! 所有落方块都过 `SkylightTracker`，森林才不会糊成黑块。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{FoliageCoordinates, f};
use crate::level::generator::world::GenWorld;

/// 对应 `TreeRTGQuercusRobur`。
pub struct TreeQuercusRobur {
    pub base: TreeRTG,
    pub height_limit: i32,
    pub height: i32,
    pub height_attenuation: f64,
    pub branch_slope: f64,
    pub scale_width: f64,
    pub leaf_density: f64,
    /// 树干粗细：1 是单干，2 是 2×2
    pub robur_trunk_size: i32,
    pub height_limit_limit: i32,
    pub leaf_distance_limit: i32,
    foliage_locations: Vec<FoliageCoordinates>,
}

impl Default for TreeQuercusRobur {
    /// 对应 `constructTreeRTGQuercusRobur0`。
    fn default() -> Self {
        Self {
            base: TreeRTG {
                log_block: 272,
                leaves_block: 296,
                crown_size: 8,
                ..TreeRTG::default()
            },
            height_limit: 0,
            height: 0,
            height_attenuation: 0.618,
            branch_slope: 0.381,
            scale_width: 1.0,
            leaf_density: 1.0,
            robur_trunk_size: 4,
            height_limit_limit: 12,
            leaf_distance_limit: 4,
            foliage_locations: Vec::new(),
        }
    }
}

impl Tree for TreeQuercusRobur {
    fn estimated_size(&self) -> f64 {
        let branch_length = f((self.base.crown_size / 4 + 2) as f64);
        f(f(branch_length * branch_length) / 16.0)
    }

    fn furthest_likely_extension(&self) -> i32 {
        self.base.crown_size / 2 + 2
    }

    fn base_mut(&mut self) -> &mut TreeRTG {
        &mut self.base
    }

    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool {
        // 树内部再派生一个随机源，保证同一棵树的形状只由这一次 nextLong 决定
        let mut tree_rand = JavaRandom::new(rand.next_long());

        if !self.base.is_ground_valid_n2(world, pos) {
            return false;
        }
        if self.height_limit == 0 {
            self.height_limit = 5 + tree_rand.next_int_bound(self.height_limit_limit);
        }
        if !self.valid_tree_location(world, pos) {
            return false;
        }

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );
        self.generate_leaf_node_list(world, &mut tree_rand, pos);
        self.generate_trunk(world, &mut tracker, pos);
        self.generate_leaves(world, &mut tracker);
        self.generate_leaf_node_bases(world, &mut tracker, pos);
        true
    }
}

impl TreeQuercusRobur {
    /// 对应 `generateLeafNodeList`：把整棵树的"叶团坐标"一次性算好。
    fn generate_leaf_node_list(&mut self, world: &GenWorld, rand: &mut JavaRandom, base_pos: Pos) {
        // 注意用的是 roburTrunkSize（树干粗细），不是 TreeRTG::trunkSize
        self.height_limit = self.robur_trunk_size + self.base.crown_size;
        self.height = (self.height_limit as f64 * self.height_attenuation) as i32;
        if self.height >= self.height_limit {
            self.height = self.height_limit - 1;
        }

        let mut branches =
            (1.382 + ((self.leaf_density * self.height_limit as f64) / 13.0).powf(2.0)) as i32;
        if branches < 1 {
            branches = 1;
        }
        let trunk_top = base_pos.y + self.height;
        let mut layer = self.height_limit - self.leaf_distance_limit;

        self.foliage_locations.clear();
        self.foliage_locations.push(FoliageCoordinates {
            pos: base_pos.up_n(layer),
            height_base: trunk_top,
        });

        while layer >= 0 {
            let size = self.layer_size(layer);
            if size >= 0.0 {
                for _ in 0..branches {
                    let radius = self.scale_width * size * (rand.next_float() as f64 + 0.328);
                    let angle = f((rand.next_float() as f64) * 2.0) * std::f64::consts::PI;
                    let dx = radius * angle.sin() + 0.5;
                    let dz = radius * angle.cos() + 0.5;
                    let foliage = base_pos.add_f64(dx, (layer - 1) as f64, dz);
                    let foliage_top = foliage.up_n(self.leaf_distance_limit);

                    if self.check_block_line(world, foliage, foliage_top) == -1 {
                        let i = base_pos.x - foliage.x;
                        let j = base_pos.z - foliage.z;
                        let drop =
                            foliage.y as f64 - ((i * i + j * j) as f64).sqrt() * self.branch_slope;
                        let height = if drop > trunk_top as f64 {
                            trunk_top
                        } else {
                            drop as i32
                        };
                        let root = Pos::new(base_pos.x, height, base_pos.z);
                        if self.check_block_line(world, root, foliage) == -1 {
                            self.foliage_locations.push(FoliageCoordinates {
                                pos: foliage,
                                height_base: root.y,
                            });
                        }
                    }
                }
            }
            layer -= 1;
        }
    }

    /// 对应 `generateTrunk`。
    fn generate_trunk(&self, world: &mut GenWorld, tracker: &mut SkylightTracker, base_pos: Pos) {
        let top = base_pos.up_n(self.height);
        let log = self.base.log_block;
        self.place_log_line(world, tracker, base_pos, top, log);
        if self.robur_trunk_size == 2 {
            self.place_log_line(world, tracker, base_pos.east(), top.east(), log);
            self.place_log_line(
                world,
                tracker,
                base_pos.east().south(),
                top.east().south(),
                log,
            );
            self.place_log_line(world, tracker, base_pos.south(), top.south(), log);
        }
    }

    /// 对应 `generateLeafNodeBases`：从树干给每个叶团拉一条斜枝。
    ///
    /// PHP 里的 `!$blockpos->equals($foliage)` 永远为真（`equals` 只认同类型），
    /// 所以这里只剩 `leafNodeNeedsBase` 判断。
    fn generate_leaf_node_bases(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        base_pos: Pos,
    ) {
        for foliage in &self.foliage_locations {
            let root = Pos::new(base_pos.x, foliage.height_base, base_pos.z);
            if self.leaf_node_needs_base(foliage.height_base - base_pos.y) {
                self.place_log_line(world, tracker, root, foliage.pos, self.base.log_block);
            }
        }
    }

    /// 对应 `generateLeaves`。
    fn generate_leaves(&self, world: &mut GenWorld, tracker: &mut SkylightTracker) {
        let leaves = block_rules::with_property(self.base.leaves_block, "check_decay", 0)
            .unwrap_or(self.base.leaves_block);
        for foliage in &self.foliage_locations {
            for step in 0..self.leaf_distance_limit {
                let radius = self.leaf_size(step);
                self.place_leaves_circle(world, tracker, foliage.pos.up_n(step), radius, leaves);
            }
        }
    }

    /// 对应 `func_181631_aN4`：在一个水平圆盘里铺树叶（只覆盖空气和已有树叶）。
    fn place_leaves_circle(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        center: Pos,
        radius: f64,
        leaves: i32,
    ) {
        let range = (radius + 0.618) as i32;
        for dx in -range..=range {
            for dz in -range..=range {
                let square = (dx.abs() as f64 + 0.5).powi(2) + (dz.abs() as f64 + 0.5).powi(2);
                if square <= f(radius * radius) {
                    let target = center.add(dx, 0, dz);
                    let state = world.block_state(target.x, target.y, target.z);
                    if state >> 4 == 0 || matches!(state >> 4, 18 | 161) {
                        self.base.place_leaves_block(world, tracker, target, leaves);
                    }
                }
            }
        }
    }

    /// 对应 `func_175937_aN4`：从 `from` 到 `to` 铺一条原木线。
    fn place_log_line(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        from: Pos,
        to: Pos,
        log: i32,
    ) {
        let delta = to.add(-from.x, -from.y, -from.z);
        let steps = self.greatest_distance(delta);
        let dx = f(delta.x as f64 / steps as f64);
        let dy = f(delta.y as f64 / steps as f64);
        let dz = f(delta.z as f64 / steps as f64);
        for step in 0..=steps {
            let target = from.add_f64(
                f(0.5 + f(f(step as f64) * dx)),
                f(0.5 + f(f(step as f64) * dy)),
                f(0.5 + f(f(step as f64) * dz)),
            );
            let axis = self.log_axis(from, target);
            if tracker.test_place(world, target, log, self.base.generate_flag) {
                let placed = block_rules::with_property(log, "log_axis", axis).unwrap_or(log);
                world.set_block_state(target.x, target.y, target.z, placed);
            }
        }
    }

    /// 对应 `checkBlockLine`：能一路穿过去就返回 -1，否则返回挡住的步数。
    fn check_block_line(&self, world: &GenWorld, from: Pos, to: Pos) -> i32 {
        let delta = to.add(-from.x, -from.y, -from.z);
        let steps = self.greatest_distance(delta);
        let dx = f(delta.x as f64 / steps as f64);
        let dy = f(delta.y as f64 / steps as f64);
        let dz = f(delta.z as f64 / steps as f64);
        if steps == 0 {
            return -1;
        }
        for step in 0..=steps {
            let target = from.add_f64(
                f(0.5 + f(f(step as f64) * dx)),
                f(0.5 + f(f(step as f64) * dy)),
                f(0.5 + f(f(step as f64) * dz)),
            );
            if !self.is_replaceable_at(world, target) {
                return step;
            }
        }
        -1
    }

    /// 对应 `validTreeLocation`。
    fn valid_tree_location(&mut self, world: &GenWorld, base_pos: Pos) -> bool {
        let state = world.block_state(base_pos.x, base_pos.y - 1, base_pos.z);
        if !matches!(state >> 4, 2 | 3 | 60) {
            return false;
        }
        let blocked = self.check_block_line(world, base_pos, base_pos.up_n(self.height_limit - 1));
        if blocked == -1 {
            true
        } else if blocked < 6 {
            false
        } else {
            // 头顶不够高就长到挡住的地方为止
            self.height_limit = blocked;
            true
        }
    }

    /// 对应 `isReplaceableAt`。
    fn is_replaceable_at(&self, world: &GenWorld, pos: Pos) -> bool {
        let id = world.block_state(pos.x, pos.y, pos.z) >> 4;
        id == 0 || matches!(id, 18 | 161) || id == 6 || matches!(id, 17 | 162)
    }

    /// 对应 `layerSize`：某一层的树冠圆盘半径。
    fn layer_size(&self, layer: i32) -> f64 {
        if f(layer as f64) < f(f(self.height_limit as f64) * 0.3) {
            return -1.0;
        }
        let half = f(f(self.height_limit as f64) / 2.0);
        let offset = f(half - f(layer as f64));
        let mut size = f((f(half * half) - f(offset * offset)).sqrt());
        if offset == 0.0 {
            size = half;
        } else if offset.abs() >= half {
            return 0.0;
        }
        f(size * 0.5)
    }

    /// 对应 `leafSize`：叶团竖向的半径（中间胖、两头瘦）。
    fn leaf_size(&self, step: i32) -> f64 {
        if step >= 0 && step < self.leaf_distance_limit {
            if step != 0 && step != self.leaf_distance_limit - 1 {
                3.0
            } else {
                2.0
            }
        } else {
            -1.0
        }
    }

    /// 对应 `leafNodeNeedsBase`：太靠下的叶团不连枝条。
    fn leaf_node_needs_base(&self, height: i32) -> bool {
        height as f64 >= self.height_limit as f64 * 0.2
    }

    /// 对应 `getGreatestDistance`：切比雪夫距离。
    fn greatest_distance(&self, delta: Pos) -> i32 {
        let dx = delta.x.abs();
        let dy = delta.y.abs();
        let dz = delta.z.abs();
        if dz > dx && dz > dy {
            dz
        } else if dy > dx {
            dy
        } else {
            dx
        }
    }

    /// 对应 `func_175938_b`：原木朝向（4 = 沿 x，8 = 沿 z，0 = 竖直）。
    fn log_axis(&self, from: Pos, to: Pos) -> i32 {
        let dx = (to.x - from.x).abs();
        let dz = (to.z - from.z).abs();
        let longest = dx.max(dz);
        if longest > 0 {
            if dx == longest {
                4
            } else if dz == longest {
                8
            } else {
                0
            }
        } else {
            0
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
    fn grows_a_full_oak() {
        let mut world = flat_world();
        let mut tree = TreeQuercusRobur::default();
        let mut rand = JavaRandom::new(42);
        assert!(tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));

        let (mut logs, mut leaves, mut top) = (0, 0, 0);
        for x in -16..32 {
            for z in -16..32 {
                for y in 0..128 {
                    match world.block(x, y, z) {
                        17 => {
                            logs += 1;
                            top = top.max(y);
                        }
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(logs >= 5, "树干原木太少：{logs}");
        assert!(leaves > 80, "树叶太少：{leaves}");
        assert!(top > 70, "树高应该超过 70，实际 {top}");
    }

    #[test]
    fn refuses_stone_ground() {
        let mut world = flat_world();
        world.set_block_state(8, 63, 8, 1 << 4);
        let mut tree = TreeQuercusRobur::default();
        let mut rand = JavaRandom::new(42);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }

    /// 打印侧视剖面，看树枝和树冠的层次。
    #[test]
    #[ignore]
    fn print_oak_profile() {
        let mut world = flat_world();
        let mut tree = TreeQuercusRobur::default();
        let mut rand = JavaRandom::new(5);
        tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8));

        for y in (60..100).rev() {
            let mut row = String::new();
            for x in -2..20 {
                let mut cell = '.';
                for z in -2..20 {
                    match world.block(x, y, z) {
                        17 => {
                            cell = '#';
                            break;
                        }
                        18 => cell = '*',
                        _ => {}
                    }
                }
                row.push(cell);
            }
            if row.chars().any(|c| c != '.') {
                println!("{y:3} {row}");
            }
        }
    }
}
