//! 对应 `rtg/deco/DecoVariable{Tree,MaterialTree,Oak,Birch,Spruce}.php`。
//!
//! 变量树先按区块噪声和局部随机数求实际高度，再依次选择大、中、小、vanilla
//! 小树或灌木。材质树在 Forest 中按噪声选择 Oak/Birch/Spruce；第一次选到 Oak
//! 时必须再选一次，这个额外的 RNG 消耗不能省略。

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    ChunkInfo, Deco, DecoTree, offset_pos, surface_height, tweak_tree_leaves,
};
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::vanilla::{
    generate_shrub, generate_vanilla_tree,
};
use crate::level::generator::realistic_terrain::tree::{
    Tree, TreeBetulaNigra, TreeBetulaPendula, TreeBetulaUtilis, TreeCupressusSempervirens,
    TreeDensityLimiter, TreeMaterials, TreePiceaSitchensis, TreePinusPonderosa, TreeQuercusFalcata,
    TreeQuercusNigra, TreeQuercusRobur, TreeVachelliaTortilis, TreeVachelliaTortilisMega, f,
};
use crate::level::generator::world::GenWorld;

const TREE_MATERIALS_SIMPLEX: usize = 7;
const TREE_MATERIALS_NOISE_DIVISOR: f64 = 631.0;

/// 对应 `TreeMaterialsChooser` 的三个来源实例。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeMaterialsChooser {
    BirchForest,
    OakForest,
    SpruceForest,
}

impl TreeMaterialsChooser {
    pub fn in_birch_forest() -> Self {
        Self::BirchForest
    }

    pub fn in_oak_forest() -> Self {
        Self::OakForest
    }

    pub fn in_spruce_forest() -> Self {
        Self::SpruceForest
    }

    /// 对应 `materialFor`：噪声和一次 `nextFloat` 共同决定材质。
    pub fn material_for(
        self,
        pos: Pos,
        rtg_world: &RtgWorld,
        rand: &mut JavaRandom,
    ) -> TreeMaterials {
        let divisor = f(TREE_MATERIALS_NOISE_DIVISOR);
        let x = f(f(pos.x as f64) / divisor);
        let z = f(f(pos.z as f64) / divisor);
        let raw_noise = f(rtg_world
            .simplex_instance(TREE_MATERIALS_SIMPLEX)
            .noise2f(x as f32, z as f32) as f64);
        let value = match self {
            Self::BirchForest => {
                f(f(f(-raw_noise) + f(rand.next_float() as f64 * f(1.0))) + f(1.0))
            }
            Self::OakForest => f(f(raw_noise + f(rand.next_float() as f64 * f(1.4))) - f(0.7)),
            Self::SpruceForest => {
                f(f(f(-raw_noise) - f(rand.next_float() as f64 * f(1.0))) - f(1.0))
            }
        };
        TreeMaterials::for_noise(value)
    }
}

/// 对应 `DecoVariableTree`；`base` 保存继承自 `DecoTree` 的散布配置。
pub struct DecoVariableTree {
    pub base: DecoTree,
    pub tall_tree: Box<dyn Tree>,
    pub medium_tree: Box<dyn Tree>,
    pub small_tree: Box<dyn Tree>,
    pub materials: TreeMaterials,
    pub tall_tree_minimum_height: i32,
    pub tall_tree_minimum_variability: i32,
    pub medium_tree_minimum_height: i32,
    pub medium_tree_minimum_variability: i32,
    pub small_tree_minimum_height: i32,
    pub small_tree_minimum_variability: i32,
    pub vanilla_tree_minimum_height: i32,
    pub vanilla_tree_minimum_variability: i32,
    pub average_height_sqrt: f64,
    pub height_noise_variability: f64,
    pub local_height_sqrt_variability: f64,
    pub sapling_chance: f64,
}

impl DecoVariableTree {
    fn new(
        tall_tree: Box<dyn Tree>,
        medium_tree: Box<dyn Tree>,
        small_tree: Box<dyn Tree>,
        materials: TreeMaterials,
    ) -> Self {
        Self {
            base: DecoTree::default(),
            tall_tree,
            medium_tree,
            small_tree,
            materials,
            tall_tree_minimum_height: 21,
            tall_tree_minimum_variability: 9,
            medium_tree_minimum_height: 12,
            medium_tree_minimum_variability: 5,
            small_tree_minimum_height: 7,
            small_tree_minimum_variability: 3,
            vanilla_tree_minimum_height: 2,
            vanilla_tree_minimum_variability: 2,
            average_height_sqrt: f(4.4),
            height_noise_variability: f(2.0),
            local_height_sqrt_variability: f(0.25),
            sapling_chance: f(0.1),
        }
    }

    /// 对应 `DecoVariableOak`。
    pub fn oak() -> Self {
        let mut result = Self::new(
            Box::new(TreeQuercusNigra::default()),
            Box::new(TreeQuercusFalcata::default()),
            Box::new(TreeQuercusRobur::default()),
            TreeMaterials::OAK,
        );
        result.change_average_height_sqrt(0.2);
        result.change_height_noise_variability(0.2);
        result.medium_tree_minimum_height += 1;
        result.tall_tree_minimum_height += 3;
        result.tall_tree_minimum_variability += 1;
        result
    }

    /// 对应 `DecoVariableBirch`。
    pub fn birch() -> Self {
        Self::new(
            Box::new(TreeBetulaPendula::default()),
            Box::new(TreeBetulaNigra::default()),
            Box::new(TreeBetulaUtilis::default()),
            TreeMaterials::BIRCH,
        )
    }

    /// 对应 `DecoVariableSpruce`。
    pub fn spruce() -> Self {
        Self::new(
            Box::new(TreePinusPonderosa::default()),
            Box::new(TreePiceaSitchensis::default()),
            Box::new(TreeCupressusSempervirens::default()),
            TreeMaterials::SPRUCE,
        )
    }

    /// 对应 `DecoVariableAcacia`：稀树草原变量树。
    ///
    /// PHP 里 tall/medium 都是 `VachelliaTortilisMega`，small 是 `VachelliaTortilis`；
    /// 高度档位比橡木矮一截（averageHeightSqrt=3.0）。
    pub fn acacia() -> Self {
        let mut result = Self::new(
            Box::new(TreeVachelliaTortilisMega::default()),
            Box::new(TreeVachelliaTortilisMega::default()),
            Box::new(TreeVachelliaTortilis::default()),
            TreeMaterials::ACACIA,
        );
        result.medium_tree_minimum_height = 11;
        result.medium_tree_minimum_variability = 3;
        result.small_tree_minimum_height = 6;
        result.small_tree_minimum_variability = 4;
        result.average_height_sqrt = f(3.0);
        result.height_noise_variability = f(1.0);
        result
    }

    pub fn change_average_height_sqrt(&mut self, change: f64) {
        self.average_height_sqrt = f(self.average_height_sqrt + f(change));
    }

    pub fn change_height_noise_variability(&mut self, change: f64) {
        self.height_noise_variability = f(self.height_noise_variability + f(change));
    }

    pub fn largest_vanilla_tree(&self) -> i32 {
        self.small_tree_minimum_height + self.small_tree_minimum_variability - 1
    }

    pub fn smallest_sapling_height(&self) -> i32 {
        self.largest_vanilla_tree() + 1
    }

    fn loop_count(&self, noise: f64, tree_density_multiplier: f64) -> i32 {
        let mut loop_count = if self.base.strength_factor_for_loops > 0.0 {
            self.base.strength_factor_for_loops as i32
        } else {
            self.base.loops
        };
        if self.base.strength_noise_factor_for_loops {
            loop_count = noise as i32;
        }
        if self.base.strength_noise_factor_x_for_loops {
            loop_count = f(noise * self.base.strength_factor_for_loops) as i32;
        }
        if loop_count < 1 {
            return 0;
        }
        self.base
            .apply_config_multipliers(loop_count, tree_density_multiplier)
    }

    /// 对应 `doVariableGenerate`：确定实际树高后进入树型档位判断。
    pub fn do_variable_generate(
        &mut self,
        rtg_world: &RtgWorld,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        chunk_info: &mut ChunkInfo,
        column: Pos,
        y: i32,
        trees_remaining: &mut TreeDensityLimiter,
    ) {
        let average_height_index = f(chunk_info.tree_height_noise_value(rtg_world));
        let average_height_index =
            f(f(average_height_index * self.height_noise_variability) + self.average_height_sqrt);
        let random_range =
            f(f(rand.next_float() as f64 * self.local_height_sqrt_variability) * f(2.0));
        let mut actual_height_index =
            f(f(average_height_index + random_range) - self.local_height_sqrt_variability);
        if y > 70 {
            actual_height_index = f(actual_height_index - f(f((y - 70) as f64) / f(20.0)));
            if average_height_index < 0.0 {
                trees_remaining.occupy(0.3);
                return;
            }
        }
        let mut actual_height = f(actual_height_index * actual_height_index) as i32;
        if (rand.next_float() as f64) < self.sapling_chance
            && actual_height > self.vanilla_tree_minimum_height
        {
            actual_height = self.vanilla_tree_minimum_height
                + rand.next_int_bound(actual_height - self.vanilla_tree_minimum_height);
        }
        self.do_generate(world, rand, column.up_n(y), actual_height, trees_remaining);
    }

    /// 对应五档 `doGenerate`。每个 `else if` 的随机数只在前面未命中时消耗。
    fn do_generate(
        &mut self,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        pos: Pos,
        actual_height: i32,
        trees_remaining: &mut TreeDensityLimiter,
    ) {
        if actual_height
            > self.tall_tree_minimum_height
                + rand.next_int_bound(self.tall_tree_minimum_variability)
        {
            Self::generate_sized_tree(
                &mut *self.tall_tree,
                world,
                rand,
                pos,
                actual_height,
                self.materials,
                trees_remaining,
                false,
            );
        } else if actual_height
            > self.medium_tree_minimum_height
                + rand.next_int_bound(self.medium_tree_minimum_variability)
        {
            Self::generate_sized_tree(
                &mut *self.medium_tree,
                world,
                rand,
                pos,
                actual_height,
                self.materials,
                trees_remaining,
                false,
            );
        } else if actual_height
            > self.small_tree_minimum_height
                + rand.next_int_bound(self.small_tree_minimum_variability)
        {
            Self::generate_sized_tree(
                &mut *self.small_tree,
                world,
                rand,
                pos,
                actual_height,
                self.materials,
                trees_remaining,
                true,
            );
        } else if actual_height
            > self.vanilla_tree_minimum_height
                + rand.next_int_bound(self.vanilla_tree_minimum_variability)
        {
            if trees_remaining.allowed(0.5, rand) {
                generate_vanilla_tree(world, rand, pos, self.materials);
            }
        } else if trees_remaining.allowed(0.7, rand) {
            generate_shrub(world, rand, pos, actual_height, self.materials);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_sized_tree(
        tree: &mut dyn Tree,
        world: &mut GenWorld,
        rand: &mut JavaRandom,
        pos: Pos,
        actual_height: i32,
        materials: TreeMaterials,
        trees_remaining: &mut TreeDensityLimiter,
        small: bool,
    ) {
        let (lowest_proportion, variability, reserve) = {
            let base = tree.base_mut();
            (
                base.lowest_variable_trunk_proportion,
                base.trunk_proportion_variability,
                base.trunk_reserve,
            )
        };
        let proportion = f(lowest_proportion + f(rand.next_float() as f64 * variability));
        let mut trunk = f(proportion * f((actual_height - reserve) as f64)) as i32 + reserve;
        if trunk < 4 {
            trunk = 4;
        }
        {
            let base = tree.base_mut();
            base.log_block = materials.log;
            base.leaves_block = materials.leaves;
            base.branch_block = materials.branches;
            base.trunk_size = trunk;
        }
        tree.set_crown_size(actual_height - trunk + if small { 2 } else { 0 });
        tree.base_mut().no_leaves = false;
        if trees_remaining.allowed(tree.estimated_size(), rand) {
            tree.generate(world, rand, pos);
        }
    }
}

impl Deco for DecoVariableTree {
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
        let offset = offset_pos(chunk_x, chunk_z);
        let noise = f(self
            .base
            .distribution
            .value(offset, rtg_world.simplex_instance(9)));
        let loop_count = self.loop_count(noise, tree_density_multiplier);
        if loop_count < 1 {
            return;
        }
        self.base.leaves_block = tweak_tree_leaves(self.base.leaves_block, false, true);
        let mut trees_remaining = TreeDensityLimiter::new(loop_count);
        while trees_remaining.not_done() {
            let pos = offset.add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            let y = surface_height(world, pos.x, pos.z);
            if y <= self.base.max_y
                && y >= self.base.min_y
                && self.base.is_valid_tree_condition(noise, rand)
            {
                if has_village {
                    let farmland = world.block_state(pos.x, pos.y - 1, pos.z) >> 4 == 60;
                    if farmland || !block_rules::area_is_air(world, pos, 2) {
                        return;
                    }
                }
                self.do_variable_generate(
                    rtg_world,
                    world,
                    rand,
                    chunk_info,
                    pos,
                    y,
                    &mut trees_remaining,
                );
            } else {
                trees_remaining.allowed(1.0, rand);
            }
        }
    }

    fn properly_defined(&self) -> bool {
        true
    }
}

/// 对应 `DecoVariableMaterialTree`。
pub struct DecoVariableMaterialTree {
    pub base: DecoTree,
    pub oak_tree: DecoVariableTree,
    pub spruce_tree: DecoVariableTree,
    pub birch_tree: DecoVariableTree,
    pub chooser: TreeMaterialsChooser,
}

impl DecoVariableMaterialTree {
    pub fn new(chooser: TreeMaterialsChooser) -> Self {
        Self {
            base: DecoTree::default(),
            oak_tree: DecoVariableTree::oak(),
            spruce_tree: DecoVariableTree::spruce(),
            birch_tree: DecoVariableTree::birch(),
            chooser,
        }
    }

    pub fn change_average_height_sqrt(&mut self, change: f64) {
        self.oak_tree.change_average_height_sqrt(change);
        self.spruce_tree.change_average_height_sqrt(change);
        self.birch_tree.change_average_height_sqrt(change);
    }

    pub fn change_height_noise_variability(&mut self, change: f64) {
        self.oak_tree.change_height_noise_variability(change);
        self.spruce_tree.change_height_noise_variability(change);
        self.birch_tree.change_height_noise_variability(change);
    }

    fn loop_count(&self, noise: f64, tree_density_multiplier: f64) -> i32 {
        let mut loop_count = if self.base.strength_factor_for_loops > 0.0 {
            self.base.strength_factor_for_loops as i32
        } else {
            self.base.loops
        };
        if self.base.strength_noise_factor_for_loops {
            loop_count = noise as i32;
        }
        if self.base.strength_noise_factor_x_for_loops {
            loop_count = f(noise * self.base.strength_factor_for_loops) as i32;
        }
        if loop_count < 1 {
            return 0;
        }
        self.base
            .apply_config_multipliers(loop_count, tree_density_multiplier)
    }
}

impl Deco for DecoVariableMaterialTree {
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
        let offset = offset_pos(chunk_x, chunk_z);
        let noise = f(self
            .base
            .distribution
            .value(offset, rtg_world.simplex_instance(9)));
        let loop_count = self.loop_count(noise, tree_density_multiplier);
        if (loop_count as f64) < 0.3 {
            return;
        }
        self.base.leaves_block = tweak_tree_leaves(self.base.leaves_block, false, true);
        let mut tries = 0;
        let mut trees_remaining = TreeDensityLimiter::new(loop_count);
        while trees_remaining.not_done() {
            let pos = offset.add(rand.next_int_bound(16), 0, rand.next_int_bound(16));
            tries += 1;
            assert!(tries <= 100, "RTG variable-tree limiter did not converge");
            let y = surface_height(world, pos.x, pos.z);
            if y <= self.base.max_y
                && y >= self.base.min_y
                && self.base.is_valid_tree_condition(noise, rand)
            {
                if has_village {
                    let farmland = world.block_state(pos.x, pos.y - 1, pos.z) >> 4 == 60;
                    if farmland || !block_rules::area_is_air(world, pos, 2) {
                        return;
                    }
                }
                let mut materials = self.chooser.material_for(pos, rtg_world, rand);
                if materials == TreeMaterials::OAK {
                    materials = self.chooser.material_for(pos, rtg_world, rand);
                }
                if materials == TreeMaterials::OAK {
                    self.oak_tree.do_variable_generate(
                        rtg_world,
                        world,
                        rand,
                        chunk_info,
                        pos,
                        y,
                        &mut trees_remaining,
                    );
                } else if materials == TreeMaterials::BIRCH {
                    self.birch_tree.do_variable_generate(
                        rtg_world,
                        world,
                        rand,
                        chunk_info,
                        pos,
                        y,
                        &mut trees_remaining,
                    );
                } else if materials == TreeMaterials::SPRUCE {
                    self.spruce_tree.do_variable_generate(
                        rtg_world,
                        world,
                        rand,
                        chunk_info,
                        pos,
                        y,
                        &mut trees_remaining,
                    );
                } else {
                    panic!("Unsupported RTG tree materials");
                }
            } else {
                trees_remaining.allowed(1.0, rand);
            }
        }
    }

    fn properly_defined(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn flat_world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        for chunk_x in -2..=2 {
            for chunk_z in -2..=2 {
                chunks.insert((chunk_x, chunk_z), Chunk::empty(chunk_x, chunk_z));
            }
        }
        let mut world = GenWorld::new(chunks);
        for x in -32..32 {
            for z in -32..32 {
                world.set_block_state(x, 63, z, 32);
            }
        }
        world
    }

    fn assert_variable_tree_grows(mut deco: DecoVariableTree, actual_height: i32) {
        let mut world = flat_world();
        let mut rand = JavaRandom::new(1000 + actual_height as i64);
        let mut limiter = TreeDensityLimiter::new(100);
        deco.do_generate(
            &mut world,
            &mut rand,
            Pos::new(0, 64, 0),
            actual_height,
            &mut limiter,
        );
        assert_eq!(
            world.block_state(0, 64, 0) >> 4,
            17,
            "高度 {actual_height} 的 {} 树应生成主干",
            deco.materials.name
        );
    }

    #[test]
    fn oak_height_parameters_match_reference() {
        let oak = DecoVariableTree::oak();
        assert_eq!(oak.average_height_sqrt, f(f(4.4) + f(0.2)));
        assert_eq!(oak.height_noise_variability, f(f(2.0) + f(0.2)));
        assert_eq!(oak.tall_tree_minimum_height, 24);
        assert_eq!(oak.tall_tree_minimum_variability, 10);
        assert_eq!(oak.medium_tree_minimum_height, 13);
        assert_eq!(oak.largest_vanilla_tree(), 9);
        assert_eq!(oak.smallest_sapling_height(), 10);
    }

    #[test]
    fn all_material_sets_have_their_reference_trees() {
        let birch = DecoVariableTree::birch();
        assert_eq!(birch.materials, TreeMaterials::BIRCH);
        assert_eq!(birch.tall_tree_minimum_height, 21);
        let spruce = DecoVariableTree::spruce();
        assert_eq!(spruce.materials, TreeMaterials::SPRUCE);
        assert_eq!(spruce.medium_tree_minimum_height, 12);
    }

    #[test]
    fn every_material_and_size_generates_a_trunk() {
        for height in [40, 20, 10] {
            assert_variable_tree_grows(DecoVariableTree::oak(), height);
            assert_variable_tree_grows(DecoVariableTree::birch(), height);
            assert_variable_tree_grows(DecoVariableTree::spruce(), height);
        }
    }

    #[test]
    fn vanilla_fallback_generates_a_tree() {
        let mut world = flat_world();
        let mut rand = JavaRandom::new(1004);
        let mut limiter = TreeDensityLimiter::new(100);
        DecoVariableTree::oak().do_generate(
            &mut world,
            &mut rand,
            Pos::new(0, 64, 0),
            4,
            &mut limiter,
        );

        assert_eq!(world.block_state(0, 64, 0) >> 4, 17);
        assert_eq!(world.block_state(0, 63, 0), 48, "种树后草方块应变成泥土");
        assert!((65..71).any(|y| world.block_state(0, y, 0) >> 4 == 17));
    }

    #[test]
    fn shrub_fallback_generates_on_soil() {
        let mut world = flat_world();
        for x in -8..=8 {
            for z in -8..=8 {
                world.set_block_state(x, 62, z, 48);
            }
        }
        let mut rand = JavaRandom::new(1002);
        let mut limiter = TreeDensityLimiter::new(100);
        DecoVariableTree::oak().do_generate(
            &mut world,
            &mut rand,
            Pos::new(0, 64, 0),
            2,
            &mut limiter,
        );

        let mut logs = 0;
        let mut leaves = 0;
        for x in -4..=4 {
            for z in -4..=4 {
                for y in 63..=65 {
                    match world.block_state(x, y, z) >> 4 {
                        17 => logs += 1,
                        18 => leaves += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(logs > 0, "灌木回退应生成原木");
        assert!(leaves > 0, "灌木回退应生成树叶");
    }

    #[test]
    fn forest_deco_generates_over_a_flat_chunk() {
        let rtg_world = RtgWorld::new(4242);
        let mut world = flat_world();
        let mut rand = JavaRandom::new(99);
        let mut chunk_info = ChunkInfo::new(0, 0);
        let mut decos =
            crate::level::generator::realistic_terrain::deco::collections::forest_decos();
        decos[0].generate(
            &rtg_world,
            &mut world,
            &mut rand,
            0,
            0,
            1.0,
            false,
            &mut chunk_info,
        );

        let mut logs = 0;
        for x in -16..16 {
            for z in -16..16 {
                for y in 64..128 {
                    if matches!(world.block_state(x, y, z) >> 4, 17 | 162) {
                        logs += 1;
                    }
                }
            }
        }
        assert!(logs > 0, "Forest 变量树链应至少生成一根原木");
    }

    #[test]
    fn material_noise_thresholds_match_picker() {
        assert_eq!(TreeMaterials::for_noise(-1.0), TreeMaterials::SPRUCE);
        assert_eq!(TreeMaterials::for_noise(-0.999), TreeMaterials::OAK);
        assert_eq!(TreeMaterials::for_noise(0.999), TreeMaterials::OAK);
        assert_eq!(TreeMaterials::for_noise(1.0), TreeMaterials::BIRCH);
    }
}
