//! 对应 `rtg/deco/DecoTree.php`：单种树的散布规则。
//!
//! 一个 `DecoTree` 描述"这种树在这个区块里怎么撒"：
//!
//! - `distribution` 给出与位置相关的数量系数（噪声 ÷ 除数 × 系数 + 偏移）
//! - `loops` / `strength_factor_for_loops` 决定尝试几次
//! - `tree_condition*` 是门限：噪声要过线，再掷一次 `nextInt(chance) == 0`
//! - 命中后按地面高度决定树的尺寸，交给具体的 `Tree` 实现去长
//!
//! 注意 RNG 消耗顺序必须逐字对齐：`nextInt` 和 `nextFloat` 的调用次数一旦变化，
//! 整片森林的布局就和参考实现分叉了。

use crate::level::generator::population::Pos;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, Distribution, get_ranged_random, offset_pos, surface_height, tweak_tree_leaves,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::Tree;
use crate::level::generator::world::GenWorld;

/// 对应 `DecoTree::$MAX_TREE_DENSITY`。
pub const MAX_TREE_DENSITY: f64 = 5.0;

/// 树的散布规则。`tree` 为 `None` 时代表"还没绑定树种"（PHP 里是 `treeType != 0`
/// 或 `tree === null`）。
pub struct DecoTree {
    pub loops: i32,
    pub strength_factor_for_loops: f64,
    pub strength_noise_factor_for_loops: bool,
    pub strength_noise_factor_x_for_loops: bool,
    pub tree_type: i32,
    pub tree: Option<Box<dyn Tree>>,
    pub distribution: Distribution,
    pub tree_condition: i32,
    pub tree_condition_noise: f64,
    pub tree_condition_noise2: f64,
    pub tree_condition_chance: i32,
    pub tree_condition_float: f64,
    pub min_y: i32,
    pub max_y: i32,
    pub log_block: i32,
    pub leaves_block: i32,
    pub min_trunk_size: i32,
    pub max_trunk_size: i32,
    pub min_crown_size: i32,
    pub max_crown_size: i32,
    pub no_leaves: bool,
}

impl Default for DecoTree {
    /// 对应 `constructDecoTree0`。
    fn default() -> Self {
        Self {
            loops: 1,
            strength_factor_for_loops: 0.0,
            strength_noise_factor_for_loops: false,
            strength_noise_factor_x_for_loops: false,
            tree_type: 0,
            tree: None,
            distribution: Distribution::new(100.0, 5.0, 0.8),
            tree_condition: 1,
            tree_condition_noise: 0.0,
            tree_condition_noise2: 0.0,
            tree_condition_chance: 1,
            tree_condition_float: 0.0,
            min_y: 63,
            max_y: 230,
            log_block: 272,
            leaves_block: 296,
            min_trunk_size: 2,
            max_trunk_size: 4,
            min_crown_size: 2,
            max_crown_size: 4,
            no_leaves: false,
        }
    }
}

impl DecoTree {
    /// 对应 `constructDecoTreeTree`：绑定一棵树，并沿用它的尺寸范围。
    pub fn from_tree(mut tree: Box<dyn Tree>) -> Self {
        let base = tree.base_mut();
        let log_block = base.log_block;
        let leaves_block = base.leaves_block;
        let min_trunk_size = base.min_trunk_size;
        let max_trunk_size = base.max_trunk_size;
        let min_crown_size = base.min_crown_size;
        let max_crown_size = base.max_crown_size;
        let no_leaves = base.no_leaves;
        Self {
            tree: Some(tree),
            log_block,
            leaves_block,
            min_trunk_size,
            max_trunk_size,
            min_crown_size,
            max_crown_size,
            no_leaves,
            ..Self::default()
        }
    }

    /// 对应 `applyConfigMultipliers`：按 biome 配置缩放树的数量。
    pub fn apply_config_multipliers(&self, loop_count: i32, multiplier: f64) -> i32 {
        (loop_count as f64 * multiplier.min(MAX_TREE_DENSITY)) as i32
    }

    /// 对应 `isValidTreeCondition`。
    pub fn is_valid_tree_condition(&self, noise: f64, rand: &mut JavaRandom) -> bool {
        match self.tree_condition {
            0 => true,
            1 => {
                noise > self.tree_condition_noise
                    && rand.next_int_bound(self.tree_condition_chance) == 0
            }
            2 => {
                noise < self.tree_condition_noise
                    && rand.next_int_bound(self.tree_condition_chance) == 0
            }
            3 => {
                // 三个条件都要算，RNG 一定消耗——不能短路
                let above_min = noise >= self.tree_condition_noise;
                let below_max = -noise <= self.tree_condition_noise2;
                let random_hit = rand.next_int_bound(self.tree_condition_chance) == 0;
                above_min && below_max && random_hit
            }
            4 => rand.next_int_bound(self.tree_condition_chance) == 0,
            5 => rand.next_int_bound(self.tree_condition_chance) != 0,
            6 => (rand.next_float() as f64) < noise,
            _ => false,
        }
    }

    /// 对应 `doGenerate`：把尺寸写在树上然后长出来。
    fn do_generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos, y: i32) {
        if self.tree_type != 0 {
            // treeType 1 走 vanilla WorldGen（如 WorldGenMegaJungle），自带尺寸逻辑。
            if let Some(tree) = self.tree.as_mut() {
                tree.generate(world, rand, pos.up_n(y));
            }
            return;
        }
        let (min_trunk, max_trunk) = (self.min_trunk_size, self.max_trunk_size);
        let (min_crown, max_crown) = (self.min_crown_size, self.max_crown_size);
        let log_block = self.log_block;
        let leaves_block = self.leaves_block;
        let no_leaves = self.no_leaves;
        if let Some(tree) = self.tree.as_mut() {
            let base = tree.base_mut();
            base.log_block = log_block;
            base.leaves_block = leaves_block;
            // 两次 nextInt 的顺序不能换
            base.trunk_size = get_ranged_random(rand, min_trunk, max_trunk);
            base.crown_size = get_ranged_random(rand, min_crown, max_crown);
            base.no_leaves = no_leaves;
            tree.generate(world, rand, pos.up_n(y));
        }
    }
}

impl Deco for DecoTree {
    /// 对应 `generate`：整个区块撒一遍这种树。
    fn generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_x: i32,
        chunk_z: i32,
        tree_density_multiplier: f64,
        has_village: bool,
        chunk_info: &mut ChunkInfo,
    ) {
        let _ = chunk_info;
        let offset = offset_pos(chunk_x, chunk_z);
        let noise = self
            .distribution
            .value(offset, rtg_world.simplex_instance(9));

        let mut loop_count = if self.strength_factor_for_loops > 0.0 {
            self.strength_factor_for_loops as i32
        } else {
            self.loops
        };
        if self.strength_noise_factor_for_loops {
            loop_count = noise as i32;
        }
        if self.strength_noise_factor_x_for_loops {
            loop_count = (noise * self.strength_factor_for_loops) as i32;
        }
        if loop_count < 1 {
            return;
        }
        loop_count = self.apply_config_multipliers(loop_count, tree_density_multiplier);
        if loop_count < 1 {
            return;
        }

        // 树叶的 check_decay/decayable 必须在放之前拨好，否则树叶会立刻消失
        self.leaves_block = tweak_tree_leaves(self.leaves_block, false, true);

        for _ in 0..loop_count {
            let pos = offset.add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            let y = surface_height(world, pos.x, pos.z);
            if y <= self.max_y && y >= self.min_y && self.is_valid_tree_condition(noise, rand) {
                if has_village {
                    return;
                }
                self.do_generate(world, rand, pos, y);
            }
        }
    }

    /// 对应 `properlyDefined`：`treeType == 0` 时必须绑定了树种。
    fn properly_defined(&self) -> bool {
        if self.tree_type == 0 {
            return self.tree.is_some();
        }
        true
    }
}
