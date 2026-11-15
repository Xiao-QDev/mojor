//! 对应 `rtg/tree/TreeRTG.php` + `rtg/feature/WorldGenAbstractTree.php`。
//!
//! 这里只有树的"公共配置 + 放置原语"，具体形状（树干怎么长、树枝怎么分叉）
//! 在各个树种自己的 `generate` 里。所有落方块都要经过 `SkylightTracker`，
//! 否则森林会长成一片黑。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::tree::f;
use crate::level::generator::realistic_terrain::tree::skylight::SkylightTracker;
use crate::level::generator::world::GenWorld;

/// 一棵树。
pub trait Tree {
    /// 在 `pos` 长一棵树，地面不允许时返回 false。
    fn generate(&mut self, world: &mut GenWorld, rand: &mut JavaRandom, pos: Pos) -> bool;

    /// 树大概占多大（给结构/村庄预留空间用）。
    fn estimated_size(&self) -> f64;

    /// 树冠最远能伸出去几格，`SkylightTracker` 的网格半径就取它。
    fn furthest_likely_extension(&self) -> i32;

    /// `DecoTree` 会在种树前按高度改写树干粗细、树冠大小等配置。
    fn base_mut(&mut self) -> &mut TreeRTG;

    /// 对应可覆写的 `setCrownSize`：部分橡树会把传入高度再减去树冠储备。
    fn set_crown_size(&mut self, size: i32) {
        self.base_mut().crown_size = size;
    }
}

/// 对应 `TreeRTG`。
#[derive(Clone, Debug)]
pub struct TreeRTG {
    pub log_block: i32,
    pub leaves_block: i32,
    pub branch_block: i32,
    pub trunk_size: i32,
    pub crown_size: i32,
    pub no_leaves: bool,
    pub sapling_block: i32,
    /// 对应 `generateFlag`：PHP 里 `setBlockState` 的通知位，Rust 侧目前不用
    pub generate_flag: i32,
    pub min_trunk_size: i32,
    pub max_trunk_size: i32,
    pub min_crown_size: i32,
    pub max_crown_size: i32,
    pub lowest_variable_trunk_proportion: f64,
    pub trunk_proportion_variability: f64,
    pub trunk_reserve: i32,
    /// 允许扎根的地面（完整 state，如 32 = 草方块）
    pub valid_ground_blocks: Vec<i32>,
    /// 允许被树穿过的材质（`BlockRules::*` 的材质常量）
    pub can_grow_into_materials: Vec<i32>,
    pub allow_bark_covered_logs: bool,
    pub max_allowed_obstruction: i32,
}

impl Default for TreeRTG {
    /// 对应 `constructTreeRTG0`。
    fn default() -> Self {
        Self {
            log_block: 17 << 4,
            // 296 = 18 << 4 | 8：默认带 check_decay
            leaves_block: (18 << 4) | 8,
            branch_block: (17 << 4) | 12,
            trunk_size: 2,
            crown_size: 4,
            no_leaves: false,
            sapling_block: 96,
            generate_flag: 19,
            min_trunk_size: 0,
            max_trunk_size: 0,
            min_crown_size: 0,
            max_crown_size: 0,
            lowest_variable_trunk_proportion: 0.25,
            trunk_proportion_variability: 0.25,
            trunk_reserve: 0,
            // 草、泥土、Java 灰化土、native 灰化土、红沙
            valid_ground_blocks: vec![32, 48, 48 | 2, 243 << 4, 192 | 1],
            can_grow_into_materials: vec![
                block_rules::AIR,
                block_rules::WOOD,
                block_rules::LEAVES,
                block_rules::GRASS,
                block_rules::GROUND,
                block_rules::PLANTS,
                block_rules::VINE,
                block_rules::WATER,
                block_rules::SNOW,
            ],
            allow_bark_covered_logs: true,
            max_allowed_obstruction: 4,
        }
    }
}

impl TreeRTG {
    /// 对应 `randomizeTreeSize`：在 min/max 之间随机一个尺寸。
    pub fn randomize_tree_size(&mut self, rand: &mut JavaRandom) {
        self.trunk_size = self.min_trunk_size
            + rand.next_int_bound(self.max_trunk_size - self.min_trunk_size + 1);
        self.crown_size = self.min_crown_size
            + rand.next_int_bound(self.max_crown_size - self.min_crown_size + 1);
    }

    /// 对应 `isReplaceable`：空气、树叶、原木、铁轨或本树允许穿透的材质都能覆盖。
    pub fn is_replaceable(&self, world: &GenWorld, pos: Pos) -> bool {
        let state = world.block_state(pos.x, pos.y, pos.z);
        let id = state >> 4;
        id == 0
            || matches!(id, 18 | 161)
            || matches!(id, 17 | 162)
            || id == 6
            || self.can_grow_into(id)
    }

    /// 对应 `canGrowInto`。木板（5）永远不允许穿过。
    pub fn can_grow_into(&self, block_id: i32) -> bool {
        if block_id == 5 {
            return false;
        }
        let material = block_rules::material(block_rules::default_state(block_id));
        self.can_grow_into_materials.contains(&material)
    }

    /// 对应 `buildTrunk`：在主干四邻补一段较粗的根盘。
    pub fn build_trunk(
        &self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        x: i32,
        y: i32,
        z: i32,
        tracker: &mut SkylightTracker,
    ) {
        let mut height = f(f(self.trunk_size as f64) / 4.0).floor() as i32;
        height = height - 2 + rand.next_int_bound(4);
        if height <= 0 {
            return;
        }
        for step in -1..height {
            self.place_trunk_block(world, tracker, Pos::new(x, y + step, z));
        }
    }

    /// 对应 `isGroundValidN2`：允许沙地。
    pub fn is_ground_valid_n2(&self, world: &GenWorld, trunk_pos: Pos) -> bool {
        self.is_ground_valid_n3(world, trunk_pos, true)
    }

    /// 对应 `isGroundValidN3`。
    pub fn is_ground_valid_n3(&self, world: &GenWorld, trunk_pos: Pos, sand_allowed: bool) -> bool {
        let ground = world.block_state(trunk_pos.x, trunk_pos.y - 1, trunk_pos.z);
        if ground >> 4 == 12 && !sand_allowed {
            return false;
        }
        self.valid_ground_blocks.contains(&ground)
    }

    /// 对应 `isGroundValidList`：树干周围每一格的地面都要合格。
    pub fn is_ground_valid_list(&self, world: &GenWorld, positions: &[Pos]) -> bool {
        assert!(!positions.is_empty(), "RTG terrain invariant: 地面列表为空");
        positions
            .iter()
            .all(|&pos| self.is_ground_valid_n2(world, pos))
    }

    /// 对应 `getTrunkLog`：树干用"树皮全覆盖"原木，看起来更粗。
    pub fn get_trunk_log(&self, default_log: i32) -> i32 {
        if !self.allow_bark_covered_logs {
            return default_log;
        }
        block_rules::with_property(default_log, "log_axis", 12).unwrap_or(default_log)
    }

    /// 对应 `placeLogBlock`。
    pub fn place_log_block(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        pos: Pos,
        log_block: i32,
    ) -> bool {
        if self.is_replaceable(world, pos) {
            return tracker.test_place(world, pos, log_block, self.generate_flag);
        }
        false
    }

    /// 对应 `placeTrunkBlock`：树干会占住整列，下面的树叶放不进来。
    pub fn place_trunk_block(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        pos: Pos,
    ) -> bool {
        if self.is_replaceable(world, pos) {
            return tracker.test_trunk(world, pos, self.log_block, self.generate_flag);
        }
        false
    }

    /// 对应 `placeLeavesBlock`。已经是同种树叶/原木时不重复放置，但也算成功。
    pub fn place_leaves_block(
        &self,
        world: &mut GenWorld,
        tracker: &mut SkylightTracker,
        pos: Pos,
        leaves_block: i32,
    ) -> bool {
        if world.is_air(pos.x, pos.y, pos.z) {
            return tracker.test_place(world, pos, leaves_block, self.generate_flag);
        }
        let state = world.block_state(pos.x, pos.y, pos.z);
        state == leaves_block || state == self.log_block || state == self.branch_block
    }

    /// 对应 `setDirtAt`：树根下面不是泥土就换成泥土。
    pub fn set_dirt_at(&self, world: &mut GenWorld, pos: Pos) {
        if world.block_state(pos.x, pos.y, pos.z) >> 4 != 3 {
            world.set_block_state(pos.x, pos.y, pos.z, 48);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        let mut world = GenWorld::new(chunks);
        for x in 0..16 {
            for z in 0..16 {
                world.set_block_state(x, 63, z, 32); // 草方块
            }
        }
        world
    }

    #[test]
    fn default_ground_is_grass_and_dirt() {
        let tree = TreeRTG::default();
        let grass = world();
        assert!(tree.is_ground_valid_n2(&grass, Pos::new(0, 64, 0)));

        // 默认 validGroundBlocks 只收红沙（192|1），普通沙不算地面
        let mut red_sand = world();
        red_sand.set_block_state(0, 63, 0, (12 << 4) | 1);
        assert!(tree.is_ground_valid_n2(&red_sand, Pos::new(0, 64, 0)));
        // 传 sand_allowed=false 时沙子一律拒绝
        assert!(!tree.is_ground_valid_n3(&red_sand, Pos::new(0, 64, 0), false));

        let mut plain_sand = world();
        plain_sand.set_block_state(0, 63, 0, 12 << 4);
        assert!(!tree.is_ground_valid_n2(&plain_sand, Pos::new(0, 64, 0)));

        // 地形落地后灰化土是 native 243，也必须能扎根
        let mut podzol = world();
        podzol.set_block_state(0, 63, 0, 243 << 4);
        assert!(tree.is_ground_valid_n2(&podzol, Pos::new(0, 64, 0)));
    }

    #[test]
    fn stone_is_not_valid_ground() {
        let tree = TreeRTG::default();
        let mut world = world();
        world.set_block_state(1, 63, 1, 1 << 4);
        assert!(!tree.is_ground_valid_n2(&world, Pos::new(1, 64, 1)));
    }

    #[test]
    fn bark_covered_trunk_log() {
        let tree = TreeRTG::default();
        // 普通橡木 → 树皮全覆盖
        assert_eq!(tree.get_trunk_log(17 << 4), (17 << 4) | 12);
        // 已经是全皮的不变
        assert_eq!(tree.get_trunk_log((17 << 4) | 12), (17 << 4) | 12);
    }

    #[test]
    fn planks_can_never_be_replaced() {
        let tree = TreeRTG::default();
        let mut world = world();
        world.set_block_state(2, 64, 2, 5 << 4);
        assert!(!tree.is_replaceable(&world, Pos::new(2, 64, 2)));
        assert!(!tree.can_grow_into(5));
    }
}
