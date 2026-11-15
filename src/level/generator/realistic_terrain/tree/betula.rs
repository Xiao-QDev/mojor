//! 对应 `rtg/tree/TreeRTGBetulaPendula.php`：垂枝白桦。
//!
//! 这是最能看出 RTG 风格的一棵：主干笔直到顶，树冠从下往上一圈圈分叉，
//! 每根树枝沿随机水平方向爬出去，走到后段开始向下垂（垂直分量 -2.4），
//! 途中按 `leafyLogFrequency` 概率在四周撒叶子。所以白桦林是"垂下来的一片"。
//!
//! **精度**：PHP 里每个 `F::f(x)` 都是一次 float32 截断（`unpack('f', pack('f'))`），
//! 这里用 `f()` 逐个照抄。少一次多一次都会让树枝落到别的格子上。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::base::{Tree, TreeRTG};
use crate::level::generator::realistic_terrain::tree::branch::TreeBranch;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::realistic_terrain::tree::{TreeMaterials, f};
use crate::level::generator::world::GenWorld;

const PI: f64 = std::f64::consts::PI;

/// 对应 `TreeRTGBetulaPendula`。
#[derive(Clone, Debug)]
pub struct TreeBetulaPendula {
    pub base: TreeRTG,
    pub branch_lengthening: f64,
    pub shortest_branch: f64,
    pub lowest_interbranch: f64,
    pub interbranch_lengthing: f64,
    pub leafy_log_frequency: f64,
}

impl Default for TreeBetulaPendula {
    /// 对应 `constructTreeRTGBetulaPendula0`。
    fn default() -> Self {
        let materials = TreeMaterials::BIRCH;
        let base = TreeRTG {
            crown_size: 20,
            trunk_size: 5,
            branch_block: materials.branches,
            leaves_block: materials.leaves,
            log_block: materials.log,
            trunk_proportion_variability: f(0.2),
            lowest_variable_trunk_proportion: f(0.2),
            ..TreeRTG::default()
        };
        Self {
            base,
            branch_lengthening: f(0.2),
            shortest_branch: f(1.6),
            lowest_interbranch: f(0.3),
            interbranch_lengthing: f(0.02),
            leafy_log_frequency: f(0.2),
        }
    }
}

impl Tree for TreeBetulaPendula {
    fn estimated_size(&self) -> f64 {
        let branch_length = f(self.shortest_branch
            + f(f(self.branch_lengthening * f(self.base.crown_size as f64)) / 2.5));
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

        // 四邻的地面都要合格（白桦根盘大）
        let ground = [
            Pos::new(x + 1, y, z),
            Pos::new(x - 1, y, z),
            Pos::new(x, y, z + 1),
            Pos::new(x, y, z - 1),
        ];
        if !self.base.is_ground_valid_list(world, &ground) {
            return false;
        }

        let mut tracker = SkylightTracker::with_tolerance(
            self.furthest_likely_extension(),
            pos,
            world,
            self.base.max_allowed_obstruction,
        );

        // 树干：从地面往上一格一格长。trunkSize=5，所以 `$i > 5` 那段侧枝逻辑
        // 在 PHP 里永远不触发（RNG 也不消耗），这里保留同样的条件。
        for i in 0..self.base.trunk_size {
            self.base
                .place_trunk_block(world, &mut tracker, Pos::new(x, y + i, z));
            if i > 5 && rand.next_int_bound(7) == 0 {
                let mut d_x = -1 + rand.next_int_bound(3);
                let mut d_z = -1 + rand.next_int_bound(3);
                if d_x == 0 && d_z == 0 {
                    d_x = -1 + rand.next_int_bound(3);
                    d_z = -1 + rand.next_int_bound(3);
                }
                let _ = (d_x, d_z);
            }
        }

        // 树冠：主干继续往上顶到 crownSize
        for crown in 0..self.base.crown_size {
            let target = pos.up_n(crown + self.base.trunk_size);
            if !self.base.place_trunk_block(world, &mut tracker, target) {
                return false;
            }
        }

        // 顶部封顶：十字 + 上面再一层，看起来像个小尖帽
        let leaves = self.base.leaves_block;
        let top = pos.up_n(self.base.crown_size + self.base.trunk_size);
        for target in [
            top,
            top.up(),
            top.east(),
            top.west(),
            top.north(),
            top.south(),
        ] {
            self.base
                .place_leaves_block(world, &mut tracker, target, leaves);
        }
        let skirting = top.up();
        for target in [
            skirting.up(),
            skirting.east(),
            skirting.west(),
            skirting.north(),
            skirting.south(),
        ] {
            self.base
                .place_leaves_block(world, &mut tracker, target, leaves);
        }

        // 从下往上撒树枝
        let mut height = f(self.base.crown_size as f64 - 1.0);
        let mut horizontal_direction = (rand.next_float() as f64 * PI) * 2.0;
        let mut placed_branches = 0;

        while height > 0.0 {
            let ascent = f(f(0.5 + f(f(height / f(self.base.crown_size as f64)) * 0.6))
                + f(rand.next_float() as f64 * 0.2));
            let branch_length_from_top = f(self.shortest_branch
                + f(
                    f(self.branch_lengthening * f(f(self.base.crown_size as f64) - height)) * 1.25,
                ));
            let branch_length_from_bottom =
                f(self.shortest_branch + f(f(self.branch_lengthening * height) * 1.75));
            let mut branch_length = branch_length_from_top.min(branch_length_from_bottom);
            let branch_start = pos.up_n(self.base.trunk_size + height as i32 - 1);

            if branch_length < 2.0 {
                // 太短，只铺叶子
                let branch =
                    TreeBranch::new(horizontal_direction, ascent, branch_length, 2, branch_start);
                self.make_leaf_branch(world, rand, &mut tracker, &branch);
            } else if branch_length < f(4.0) {
                branch_length = f(branch_length - 1.0);
                let branch =
                    TreeBranch::new(horizontal_direction, ascent, branch_length, 2, branch_start);
                self.make_leafy_log_branch(world, rand, &mut tracker, &branch);
            } else {
                branch_length = f(branch_length - 1.5);
                branch_length = f(branch_length * 0.8);
                let branch =
                    TreeBranch::new(horizontal_direction, ascent, branch_length, 1, branch_start);
                self.make_leafy_log_branch(world, rand, &mut tracker, &branch);
            }

            horizontal_direction += f(1.85 + rand.next_float() as f64);
            if horizontal_direction > PI * 2.0 {
                horizontal_direction -= PI * 2.0;
            }
            let branch_drop =
                f(self.lowest_interbranch
                    + f(self.interbranch_lengthing * f(placed_branches as f64)));
            height = f(height - f(branch_drop * f(1.0 + rand.next_float() as f64)));
            placed_branches += 1;
        }

        true
    }
}

impl TreeBetulaPendula {
    /// 对应 `makeLeafBranch`：一路只铺叶子（细枝看不见木头）。
    fn make_leaf_branch(
        &self,
        world: &mut GenWorld,
        _rand: &mut JavaRandom,
        tracker: &mut SkylightTracker,
        branch: &TreeBranch,
    ) {
        let mut branch = branch.clone();
        let leaves = self.base.leaves_block;
        while branch.not_done() {
            let place = branch.moved_orthogonally();
            if !self.base.place_leaves_block(world, tracker, place, leaves) {
                return;
            }
        }
    }

    /// 对应 `makeLeafyLogBranch`：边走边放树枝原木，并按概率在四周撒叶子。
    fn make_leafy_log_branch(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        tracker: &mut SkylightTracker,
        branch: &TreeBranch,
    ) {
        let leaves = self.base.leaves_block;
        let branch_block = self.base.branch_block;
        let mut branch = branch.clone();
        // 同一条枝再走一遍，只铺叶子——让枝条周围变厚
        let mut leaf_connections = TreeBranch::copied(&branch);

        while branch.not_done() {
            let old_location = branch.location();
            let location = branch.moved();
            if !self
                .base
                .place_log_block(world, tracker, location, branch_block)
            {
                return;
            }

            // 树枝跨了高度：在拐角补叶子，避免"悬空直角"
            if old_location.y != location.y {
                let bridge = Pos::new(location.x, old_location.y, location.z);
                self.base.place_leaves_block(world, tracker, bridge, leaves);
                if old_location.x != location.x {
                    let bridge = Pos::new(old_location.x, old_location.y, location.z);
                    self.base.place_leaves_block(world, tracker, bridge, leaves);
                }
                if old_location.z != location.z {
                    let bridge = Pos::new(location.x, old_location.y, old_location.z);
                    self.base.place_leaves_block(world, tracker, bridge, leaves);
                }
            }

            self.base
                .place_leaves_block(world, tracker, branch.location().up(), leaves);

            // 5 次独立抽样，顺序必须是 up/east/west/north/south（RNG 消耗要对齐）
            let branch_leaf_frequency = f(self.leafy_log_frequency * f(branch.stage as f64));
            if (rand.next_float() as f64) < branch_leaf_frequency {
                self.base
                    .place_leaves_block(world, tracker, location.up(), leaves);
            }
            if (rand.next_float() as f64) < branch_leaf_frequency {
                self.base
                    .place_leaves_block(world, tracker, location.east(), leaves);
            }
            if (rand.next_float() as f64) < branch_leaf_frequency {
                self.base
                    .place_leaves_block(world, tracker, location.west(), leaves);
            }
            if (rand.next_float() as f64) < branch_leaf_frequency {
                self.base
                    .place_leaves_block(world, tracker, location.north(), leaves);
            }
            if (rand.next_float() as f64) < branch_leaf_frequency {
                self.base
                    .place_leaves_block(world, tracker, location.south(), leaves);
            }
        }

        while leaf_connections.not_done() {
            let place = leaf_connections.moved_orthogonally();
            self.base.place_leaves_block(world, tracker, place, leaves);
        }

        if branch.stage == 1 {
            // 左枝
            let direction = (branch.initial_horizontal + 0.5) + f(rand.next_float() as f64 * 0.3);
            let ascent = f(f(branch.initial_vertical - 0.4) + f(rand.next_float() as f64 * 0.8));
            let length = f(branch.initial_length * f(0.5 + f(rand.next_float() as f64 * 0.2)));
            let child = TreeBranch::new(
                direction,
                ascent,
                length,
                branch.stage + 1,
                branch.location(),
            );
            self.make_leafy_log_branch(world, rand, tracker, &child);

            // 右枝
            let direction = (branch.initial_horizontal - 0.8) + f(rand.next_float() as f64 * 0.3);
            let ascent = f(f(branch.initial_vertical - 0.4) + f(rand.next_float() as f64 * 0.8));
            let length = f(branch.initial_length * f(0.5 + f(rand.next_float() as f64 * 0.2)));
            let child = TreeBranch::new(
                direction,
                ascent,
                length,
                branch.stage + 1,
                branch.location(),
            );
            self.make_leafy_log_branch(world, rand, tracker, &child);

            // 下垂枝：这一根只铺叶子
            let direction = (branch.initial_horizontal - 0.15) + f(rand.next_float() as f64 * 0.3);
            let ascent = f(-2.4 + f(rand.next_float() as f64 * 0.8));
            let length = f(1.2 * f(0.8 + f(rand.next_float() as f64 * 0.4)));
            let child = TreeBranch::new(
                direction,
                ascent,
                length,
                branch.stage + 1,
                branch.location(),
            );
            self.make_leaf_branch(world, rand, tracker, &child);
        } else if branch.stage == 2 {
            // 末端两根短枝 + 一根下垂枝，都只铺叶子
            for (offset, base_ascent) in [
                (0.5, f(branch.initial_vertical - 0.4)),
                (-1.0, f(branch.initial_vertical - 0.4)),
            ] {
                let direction =
                    (branch.initial_horizontal + offset) + f(rand.next_float() as f64 * 0.5);
                let ascent = f(base_ascent + f(rand.next_float() as f64 * 0.4));
                let length = f(1.2 * f(0.9 + f(rand.next_float() as f64 * 0.2)));
                let child = TreeBranch::new(
                    direction,
                    ascent,
                    length,
                    branch.stage + 1,
                    branch.location(),
                );
                self.make_leaf_branch(world, rand, tracker, &child);
            }

            let direction = (branch.initial_horizontal - 0.15) + f(rand.next_float() as f64 * 0.3);
            let ascent = f(-2.4 + f(rand.next_float() as f64 * 0.8));
            let length = f(1.2 * f(0.8 + f(rand.next_float() as f64 * 0.4)));
            let child = TreeBranch::new(
                direction,
                ascent,
                length,
                branch.stage + 1,
                branch.location(),
            );
            self.make_leaf_branch(world, rand, tracker, &child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    /// 3×3 区块的平地，草面在 y=63。
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
    fn grows_a_full_birch() {
        let mut world = flat_world();
        let mut tree = TreeBetulaPendula::default();
        let mut rand = JavaRandom::new(42);
        assert!(
            tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)),
            "平地上应该能长出来"
        );

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
        // trunkSize 5 + crownSize 20 = 25 格主干
        assert!(logs >= 24, "树干原木太少：{logs}");
        assert!(leaves > 60, "树叶太少：{leaves}");
        assert!(top > 85, "树高应超过 85，实际 {top}");
    }

    #[test]
    fn refuses_stone_ground() {
        let mut world = flat_world();
        // 白桦只检查四邻地面（不含中心），所以把东侧那格换成石头
        world.set_block_state(9, 63, 8, 1 << 4);
        let mut tree = TreeBetulaPendula::default();
        let mut rand = JavaRandom::new(42);
        assert!(!tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8)));
    }

    /// 种一小片白桦林，打印树冠中部的水平切面，看整体形态。
    #[test]
    #[ignore]
    fn print_birch_forest() {
        let mut world = flat_world();
        let mut rand = JavaRandom::new(2024);
        // 故意靠近一点，看遮光预算会不会让它们互相让位
        let spots = [(2, 2), (9, 5), (15, 12), (5, 14), (17, 2), (22, 20)];
        for &(x, z) in &spots {
            let mut tree = TreeBetulaPendula::default();
            let grown = tree.generate(&mut world, &mut rand, Pos::new(x, 64, z));
            println!("({x},{z}) -> {grown}");
        }

        for y in [78, 84, 88] {
            println!("=== y={y}");
            let mut layer = String::new();
            for x in -2..30 {
                for z in -2..30 {
                    layer.push(match world.block(x, y, z) {
                        17 => '#',
                        18 => '*',
                        _ => '.',
                    });
                }
                layer.push('\n');
            }
            println!("{layer}");
        }
    }

    /// 打印每层的俯视图，肉眼确认树形（`cargo test -- --ignored --nocapture`）。
    #[test]
    #[ignore]
    fn print_birch_layers() {
        let mut world = flat_world();
        let mut tree = TreeBetulaPendula::default();
        let mut rand = JavaRandom::new(7);
        tree.generate(&mut world, &mut rand, Pos::new(8, 64, 8));

        for y in (64..110).rev() {
            let mut layer = String::new();
            for x in -2..20 {
                for z in -2..20 {
                    layer.push(match world.block(x, y, z) {
                        17 => '#',
                        18 => '*',
                        _ => '.',
                    });
                }
                layer.push('\n');
            }
            if layer.contains('#') || layer.contains('*') {
                println!("--- y={y}\n{layer}");
            }
        }
    }
}
