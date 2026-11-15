//! 对应 `rtg/deco/biome/VanillaBiomeDecos*.php`：每个 biome 的装饰清单。
//!
//! 现在只搬了 **Plains**（`VanillaBiomeDecos1`）里的树部分——花、草、小麦、
//! 甘蔗、南瓜这些还没接，所以平原会显得比参考实现空一些。
//!
//! 两个 `DecoTree` 用的是同一棵树形（`TreeRTGQuercusRobur`），只有木材不同：
//! 一个是橡木、一个是桦木。参考实现里平原确实就是这么配的，不是简化。

use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::deco::{
    Deco, DecoBoulder, DecoCactus, DecoCobwebs, DecoCrop, DecoDeadBush, DecoDoubleGrass, DecoFallenTree,
    DecoFlowersRtg, DecoHelper5050, DecoHelperRandomSplit, DecoHelperThisOrThat, DecoJungleCacti,
    DecoMushrooms, DecoPumpkin, DecoReed, DecoShrub, DecoSponge, DecoTree, DecoVariableFallenTree,
    DecoVariableMaterialTree, DecoVariableTree, DecoWorldGen, Distribution, FallenWoodland,
    TreeMaterialsChooser,
};
use crate::level::generator::realistic_terrain::tree::{
    IceSpikeRTG, TreeAcaciaBucheri, TreeCeibaPentandra, TreeCocosNucifera,
    TreeCupressusSempervirens, TreeMegaJungle, TreePiceaPungens, TreePiceaSitchensis,
    TreePinusNigra, TreePinusPonderosa, TreeQuercusRobur, TreeRhizophoraMucronata,
    TreeSalixMyrtilloides, VanillaOakTree, WorldGenBigMushroom, f,
};

/// 按 biome id 取装饰清单。未移植的 biome 返回空表。
pub fn decos_for_biome(biome_id: u8) -> Vec<Box<dyn Deco>> {
    match biome_id as i32 {
        // plains / sunflower_plains
        1 | 129 => plains_decos(),
        // desert / desert_hills
        2 | 17 | 130 => desert_decos(),
        // forest / forest_hills
        4 | 18 => forest_decos(),
        // taiga / taiga_hills / cold_taiga / cold_taiga_hills
        5 | 19 | 30 | 31 | 133 | 158 => taiga_decos(),
        // swampland
        6 => swamp_decos(70, 90),
        // swampland_m
        134 => swamp_decos(100, 100),
        // birch_forest / birch_forest_hills
        27 | 28 => birch_forest_decos(),
        // birch_forest_m / birch_forest_hills_m
        155 | 156 => birch_forest_m_decos(),
        // mega_taiga / mega_taiga_hills
        32 | 33 => mega_taiga_decos(),
        // mega_spruce_taiga（只有倒木）
        160 | 159 => mega_spruce_taiga_decos(),
        // redwood_taiga_hills（同 mega_taiga）
        161 => mega_taiga_decos(),
        // extreme_hills
        3 => extreme_hills_decos(),
        // extreme_hills_edge
        20 => extreme_hills_edge_decos(),
        // extreme_hills_plus
        34 => extreme_hills_plus_decos(),
        // extreme_hills_m
        131 => extreme_hills_m_decos(),
        // extreme_hills_plus_m
        162 => extreme_hills_plus_m_decos(),
        // savanna
        35 => savanna_decos(),
        // savanna_plateau / savanna_plateau_m
        36 | 164 => savanna_plateau_decos(),
        // savanna_m（DesertRiver + Savanna）
        163 => savanna_m_decos(),
        // jungle / jungle_hills / jungle_m
        21 | 22 | 149 => jungle_decos(),
        // jungle_edge / jungle_edge_m
        23 | 151 => jungle_edge_decos(),
        // roofed_forest / roofed_forest_m
        29 => roofed_forest_decos(),
        157 => roofed_forest_m_decos(),
        // mesa / mesa_plateau / mesa_plateau_f / mesa_bryce / mesa_plateau_f_m / mesa_plateau_m
        37 | 39 | 165 | 167 => mesa_decos(),
        38 | 166 => mesa_plateau_decos(),
        // ice_plains / ice_mountains / ice_plains_spikes
        12 | 13 | 140 => ice_decos(),
        // beach / cold_beach
        16 => beach_decos(),
        26 => cold_beach_decos(),
        // flower_forest
        132 => flower_forest_decos(),
        // mushroom_island / mushroom_island_shore：只有蘑菇
        14 | 15 => mushroom_island_decos(),
        // ocean / deep_ocean：海底巨石 + 海绵
        0 | 24 => ocean_decos(),
        // river / frozen_ocean / frozen_river / stone_beach：无 RTG 装饰
        7 | 10 | 11 | 25 => Vec::new(),
        _ => Vec::new(),
    }
}

/// 对应 `DecoCollectionDesert`：仙人掌 + 枯草。
pub fn desert_decos() -> Vec<Box<dyn Deco>> {
    let cactus = DecoCactus {
        max_y: 90,
        strength_factor: f(3.0),
        chance: 2,
        ..DecoCactus::default()
    };
    let dead_bush = DecoDeadBush {
        max_y: 128,
        strength_factor: f(2.0),
        chance: 3,
        ..DecoDeadBush::default()
    };
    vec![Box::new(cactus), Box::new(dead_bush)]
}

/// 对应 `DecoCollectionTaiga`：云杉变量树 + 枯树 + 灌木 + 倒木 + 巨石 + 南瓜 + 蘑菇。
pub fn taiga_decos() -> Vec<Box<dyn Deco>> {
    // DecoVariableTaigaTree → 用 SpruceForest 材料选择器的 DecoVariableMaterialTree 代替。
    let mut taiga_trees = DecoVariableMaterialTree::new(TreeMaterialsChooser::in_spruce_forest());
    taiga_trees.base.loops = 10;
    taiga_trees.base.max_y = 80;
    taiga_trees.base.strength_factor_for_loops = f(8.0);
    taiga_trees.base.strength_noise_factor_for_loops = true;

    let dead_cypress = DecoTree {
        loops: 1,
        min_y: 62,
        max_y: 120,
        tree_condition_chance: 20,
        tree: Some(Box::new(TreeCupressusSempervirens::default())),
        no_leaves: true,
        ..DecoTree::default()
    };
    let dead_spruce = DecoTree {
        loops: 1,
        min_y: 60,
        max_y: 120,
        tree_condition_chance: 20,
        tree: Some(Box::new(TreePiceaSitchensis::default())),
        no_leaves: true,
        ..DecoTree::default()
    };
    let shrub = DecoShrub {
        loops: 1,
        max_y: 120,
        chance: 5,
        min_size: 0,
        max_size: 3,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        ..DecoShrub::default()
    };
    let fallen = {
        let mut f = DecoVariableFallenTree::new(FallenWoodland::Spruce);
        f.max_y = 120;
        f
    };
    let boulder = DecoBoulder::default();
    let pumpkin = DecoPumpkin {
        loops: 2,
        max_y: 128,
        chance: 32,
        ..DecoPumpkin::default()
    };
    let mushrooms = DecoMushrooms {
        loops: 1,
        max_y: 128,
        chance: 8,
        ..DecoMushrooms::default()
    };

    vec![
        Box::new(taiga_trees),
        Box::new(dead_cypress),
        Box::new(dead_spruce),
        Box::new(shrub),
        Box::new(fallen),
        Box::new(boulder),
        Box::new(pumpkin),
        Box::new(mushrooms),
    ]
}

/// 对应 `DecoCollectionForest`：变量树、倒木、两种独立灌木和花。
pub fn forest_decos() -> Vec<Box<dyn Deco>> {
    let mut trees = DecoVariableMaterialTree::new(TreeMaterialsChooser::in_oak_forest());
    trees.base.strength_factor_for_loops = 6.0;
    trees.base.tree_type = 0;
    trees.base.distribution = Distribution::new(837.0, 2.5, 4.5);
    trees.base.tree_condition = 0;
    trees.base.tree_condition_noise = f(-1.0);
    trees.base.tree_condition_noise2 = f(3.0);
    trees.base.tree_condition_chance = 1;
    trees.base.max_y = 120;
    trees.base.strength_noise_factor_for_loops = true;
    trees.base.strength_noise_factor_x_for_loops = false;

    let mut logs = DecoVariableFallenTree::new(FallenWoodland::Oak);
    logs.max_y = 80;
    logs.min_size = 3;
    logs.max_size = 8;

    let mut oak_shrubs = DecoShrub::default();
    oak_shrubs.max_y = 140;
    oak_shrubs.loop_multiplier = f(2.0);
    oak_shrubs.chance = 3;

    let mut spruce_shrubs = DecoShrub::default();
    spruce_shrubs.log_block = block_rules::wood_state(1, false);
    spruce_shrubs.leaves_block = block_rules::wood_state(1, true);
    spruce_shrubs.max_y = 140;
    spruce_shrubs.loop_multiplier = f(2.0);
    spruce_shrubs.chance = 9;

    let flowers = DecoFlowersRtg {
        flowers: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
        max_y: 128,
        strength_factor: f(6.0),
        ..DecoFlowersRtg::default()
    };

    vec![
        Box::new(trees),
        Box::new(logs),
        Box::new(oak_shrubs),
        Box::new(spruce_shrubs),
        Box::new(flowers),
    ]
}

/// 对应 `VanillaBiomeDecos1::initDecos` 的树部分。
///
/// ```text
/// oakTrees:   DecoTree(TreeRTGQuercusRobur) + Distribution(100, 6, 0.8)
///             treeCondition 1, noise > 0.4, chance 48
/// birchTrees: 同上，但木材换成桦木（woodState(2, ...)）
/// ThisOrThat(4, 1, oak, birch) → 3:1
/// ```
pub fn plains_decos() -> Vec<Box<dyn Deco>> {
    let oak_trees = make_plains_tree(272, 296);
    let birch_trees = make_plains_tree(
        block_rules::wood_state(2, false),
        block_rules::wood_state(2, true),
    );
    let reed = DecoReed {
        max_y: 65,
        loops: 3,
        ..DecoReed::default()
    };
    let pumpkin = DecoPumpkin {
        max_y: 90,
        ..DecoPumpkin::default()
    };

    // 小麦斑块（对应 VanillaBiomeDecos1 的 DecoCrop，size=4/density=4/chance=50/water=false）
    let wheat = DecoCrop {
        crop_type: 3,
        farm_size: 4,
        farm_density: 4,
        chance: 50,
        water: false,
        min_y: 63,
        max_y: 255,
        ..DecoCrop::default()
    };

    vec![
        Box::new(DecoHelperThisOrThat::new(
            4,
            1,
            Box::new(oak_trees),
            Box::new(birch_trees),
        )),
        Box::new(reed),
        Box::new(pumpkin),
        Box::new(wheat),
    ]
}

/// 平原的树配置：树干 3..5、树冠 7..9，`noise > 0.4` 且 1/48 概率。
fn make_plains_tree(log_block: i32, leaves_block: i32) -> DecoTree {
    let mut tree = TreeQuercusRobur::default();
    tree.base.log_block = log_block;
    tree.base.leaves_block = leaves_block;
    tree.base.min_trunk_size = 3;
    tree.base.max_trunk_size = 5;
    tree.base.min_crown_size = 7;
    tree.base.max_crown_size = 9;

    let mut deco = DecoTree::from_tree(Box::new(tree));
    deco.tree_type = 0;
    deco.tree_condition = 1;
    deco.distribution = Distribution::new(100.0, 6.0, f(0.8));
    deco.tree_condition_noise = f(0.4);
    deco.tree_condition_chance = 48;
    deco
}

/// 对应 `VanillaBiomeDecos6`（沼泽）/`VanillaBiomeDecos134`（沼泽 M）。
///
/// 两个 biome 配置几乎一样，只有柳树和死松的 `maxY` 不同：
/// - 沼泽：柳树 70、死松 90
/// - 沼泽 M：都是 100
///
/// 组件：柳树（`TreeSalixMyrtilloides`）+ 死黄松（`TreePinusPonderosa` 无叶）
/// + 灌木 + 金合欢倒木 + 南瓜。
pub fn swamp_decos(willow_max_y: i32, dead_pine_max_y: i32) -> Vec<Box<dyn Deco>> {
    let willow = {
        let mut deco = DecoTree::from_tree(Box::new(TreeSalixMyrtilloides::default()));
        deco.strength_noise_factor_x_for_loops = true;
        deco.strength_factor_for_loops = f(1.0);
        deco.distribution = Distribution::new(80.0, 60.0, f(-15.0));
        deco.tree_type = 0;
        deco.tree_condition = 4;
        deco.tree_condition_chance = 12;
        deco.max_y = willow_max_y;
        deco
    };

    let dead_pine = {
        let mut pine = TreePinusPonderosa::default();
        pine.base.min_trunk_size = 3;
        pine.base.max_trunk_size = 6;
        pine.base.min_crown_size = 6;
        pine.base.max_crown_size = 14;
        pine.base.no_leaves = true;
        let mut deco = DecoTree::from_tree(Box::new(pine));
        deco.tree_type = 0;
        deco.tree_condition = 4;
        deco.tree_condition_chance = 18;
        deco.max_y = dead_pine_max_y;
        deco
    };

    let shrub = DecoShrub {
        max_y: 100,
        loop_multiplier: f(3.0),
        ..DecoShrub::default()
    };

    let fallen = DecoFallenTree {
        distribution: Distribution::new(80.0, 60.0, f(-15.0)),
        log_condition_chance: 6,
        log_block: block_rules::wood_state(5, false),
        leaves_block: block_rules::wood_state(5, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let pumpkin = DecoPumpkin {
        max_y: 90,
        ..DecoPumpkin::default()
    };

    vec![
        Box::new(willow),
        Box::new(dead_pine),
        Box::new(shrub),
        Box::new(fallen),
        Box::new(pumpkin),
    ]
}

/// 对应 `DecoCollectionBirchForest`（桦木林 biome 27/28）。
///
/// 组件：桦木变量树（`in_birch_forest` 选择器）+ 桦木倒木 + 橡木灌木 + 花。
/// 变量树的 `max_y` 和 `strength_noise_factor_x_for_loops` 保持 PHP 默认值（230 / false）。
pub fn birch_forest_decos() -> Vec<Box<dyn Deco>> {
    let mut trees = DecoVariableMaterialTree::new(TreeMaterialsChooser::in_birch_forest());
    trees.base.strength_factor_for_loops = f(6.0);
    trees.base.tree_type = 0;
    trees.base.distribution = Distribution::new(837.0, 2.5, 4.5);
    trees.base.tree_condition = 0;
    trees.base.tree_condition_noise = f(-1.0);
    trees.base.tree_condition_noise2 = f(3.0);
    trees.base.tree_condition_chance = 1;
    trees.base.strength_noise_factor_for_loops = true;

    let logs = DecoFallenTree {
        log_condition: 1,
        log_condition_chance: 8,
        log_block: block_rules::wood_state(2, false),
        leaves_block: block_rules::wood_state(2, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let shrub = DecoShrub {
        max_y: 120,
        loop_multiplier: f(1.5),
        ..DecoShrub::default()
    };

    let flowers = DecoFlowersRtg {
        flowers: vec![4, 7],
        max_y: 128,
        strength_factor: f(12.0),
        ..DecoFlowersRtg::default()
    };

    vec![
        Box::new(trees),
        Box::new(logs),
        Box::new(shrub),
        Box::new(flowers),
    ]
}

/// 对应 `DecoCollectionBirchForestM`（桦木林 M biome 155/156）。
///
/// 和普通桦木林的唯一区别：变量树额外调用 `changeAverageHeightSqrt(2.0)`
/// 和 `changeHeightNoiseVariability(-1.0)`，树更高更密。
pub fn birch_forest_m_decos() -> Vec<Box<dyn Deco>> {
    let mut trees = DecoVariableMaterialTree::new(TreeMaterialsChooser::in_birch_forest());
    trees.base.strength_factor_for_loops = f(6.0);
    trees.base.tree_type = 0;
    trees.base.distribution = Distribution::new(837.0, 2.5, 4.5);
    trees.base.tree_condition = 0;
    trees.base.tree_condition_noise = f(-1.0);
    trees.base.tree_condition_noise2 = f(3.0);
    trees.base.tree_condition_chance = 1;
    trees.base.strength_noise_factor_for_loops = true;
    trees.change_average_height_sqrt(f(2.0));
    trees.change_height_noise_variability(f(-1.0));

    let logs = DecoFallenTree {
        log_condition: 1,
        log_condition_chance: 8,
        log_block: block_rules::wood_state(2, false),
        leaves_block: block_rules::wood_state(2, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let shrub = DecoShrub {
        max_y: 120,
        loop_multiplier: f(1.5),
        ..DecoShrub::default()
    };

    let flowers = DecoFlowersRtg {
        flowers: vec![4, 7],
        max_y: 128,
        strength_factor: f(12.0),
        ..DecoFlowersRtg::default()
    };

    vec![
        Box::new(trees),
        Box::new(logs),
        Box::new(shrub),
        Box::new(flowers),
    ]
}

/// 对应 `VanillaBiomeDecos32`/`33`（巨型针叶林 / 巨型针叶林丘陵）。
///
/// 组件：鹅卵石巨石 + 三棵树的随机分配（蓝叶云杉 8 / 小云杉 2 / 黄松 2）
/// + 云杉倒木 + 灌木 + 蘑菇 + 南瓜。三棵树共用 `DecoHelperRandomSplit`，
/// 每个区块只跑其中一棵。
pub fn mega_taiga_decos() -> Vec<Box<dyn Deco>> {
    // 小云杉：PiceaSitchensis，trunk 4..9 / crown 5..14
    let small_pines = {
        let mut sitchensis = TreePiceaSitchensis::default();
        sitchensis.base.min_trunk_size = 4;
        sitchensis.base.max_trunk_size = 9;
        sitchensis.base.min_crown_size = 5;
        sitchensis.base.max_crown_size = 14;
        let mut deco = DecoTree::from_tree(Box::new(sitchensis));
        deco.strength_noise_factor_x_for_loops = true;
        deco.strength_factor_for_loops = f(4.0);
        deco.tree_type = 0;
        deco.tree_condition = 0;
        deco.tree_condition_chance = 3;
        deco.max_y = 100;
        deco
    };

    // 蓝叶云杉：PiceaPungens，trunk 2..7 / crown 6..17
    let spruce_trees = {
        let mut pungens = TreePiceaPungens::default();
        pungens.base.min_trunk_size = 2;
        pungens.base.max_trunk_size = 7;
        pungens.base.min_crown_size = 6;
        pungens.base.max_crown_size = 17;
        let mut deco = DecoTree::from_tree(Box::new(pungens));
        deco.strength_noise_factor_x_for_loops = true;
        deco.strength_factor_for_loops = f(3.0);
        deco.tree_type = 0;
        deco.tree_condition = 0;
        deco.tree_condition_chance = 1;
        deco.max_y = 100;
        deco
    };

    // 黄松：PinusPonderosa，橡木色，trunk 11..21 / crown 15..29
    let oak_pines = {
        let mut ponderosa = TreePinusPonderosa::default();
        ponderosa.base.min_trunk_size = 11;
        ponderosa.base.max_trunk_size = 21;
        ponderosa.base.min_crown_size = 15;
        ponderosa.base.max_crown_size = 29;
        let mut deco = DecoTree::from_tree(Box::new(ponderosa));
        deco.strength_noise_factor_x_for_loops = true;
        deco.strength_factor_for_loops = f(3.0);
        deco.tree_type = 0;
        deco.tree_condition = 0;
        deco.tree_condition_chance = 1;
        deco.max_y = 100;
        deco
    };

    let tree_split = DecoHelperRandomSplit::new(
        vec![
            Box::new(spruce_trees),
            Box::new(small_pines),
            Box::new(oak_pines),
        ],
        vec![8, 2, 2],
    );

    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 16,
        max_y: 95,
        strength_factor: f(3.0),
        ..DecoBoulder::default()
    };

    let fallen = DecoFallenTree {
        distribution: Distribution::new(100.0, 6.0, f(0.8)),
        log_condition_chance: 6,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let shrub = DecoShrub {
        max_y: 100,
        loop_multiplier: f(2.0),
        chance: 10,
        ..DecoShrub::default()
    };

    let mushrooms = DecoMushrooms {
        max_y: 90,
        ..DecoMushrooms::default()
    };

    let pumpkin = DecoPumpkin {
        max_y: 90,
        ..DecoPumpkin::default()
    };

    vec![
        Box::new(boulder),
        Box::new(tree_split),
        Box::new(fallen),
        Box::new(shrub),
        Box::new(mushrooms),
        Box::new(pumpkin),
    ]
}

/// 对应 `VanillaBiomeDecos160`（MegaSpruceTaiga）：只有一根云杉倒木。
pub fn mega_spruce_taiga_decos() -> Vec<Box<dyn Deco>> {
    let fallen = DecoFallenTree {
        distribution: Distribution::new(100.0, 6.0, f(0.8)),
        log_condition_chance: 6,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };
    vec![Box::new(fallen)]
}

// ---------------------------------------------------------------------------
// 极端丘陵家族（biome 3 / 20 / 34 / 131 / 162）
// 共用 `TreePinusNigra` 黑松 + `DecoHelper5050` 在橡木色/云杉色之间二选一。
// ---------------------------------------------------------------------------

/// 构造一棵黑松 `DecoTree`。
///
/// `strength_factor` 为 0 时用 `loops` 控制循环次数（M 变体），否则用
/// `strength_factor_for_loops`。distribution 固定 100/6/0.8，treeCondition=4。
fn nigra_tree_deco(
    log: i32,
    leaves: i32,
    max_y: i32,
    min_trunk: i32,
    max_trunk: i32,
    min_crown: i32,
    max_crown: i32,
    strength_factor: f64,
    loops: i32,
    chance: i32,
) -> DecoTree {
    let mut nigra = TreePinusNigra::default();
    nigra.base.log_block = log;
    nigra.base.leaves_block = leaves;
    nigra.base.min_trunk_size = min_trunk;
    nigra.base.max_trunk_size = max_trunk;
    nigra.base.min_crown_size = min_crown;
    nigra.base.max_crown_size = max_crown;
    let mut deco = DecoTree::from_tree(Box::new(nigra));
    if strength_factor > 0.0 {
        deco.strength_factor_for_loops = f(strength_factor);
    } else {
        deco.loops = loops;
    }
    deco.strength_noise_factor_x_for_loops = true;
    deco.distribution = Distribution::new(100.0, 6.0, f(0.8));
    deco.tree_type = 0;
    deco.tree_condition = 4;
    deco.tree_condition_chance = chance;
    deco.max_y = max_y;
    deco
}

/// 橡木色 + 云杉色黑松的 50/50 包装。
fn nigra_5050(
    max_y: i32,
    min_trunk: i32,
    max_trunk: i32,
    min_crown: i32,
    max_crown: i32,
    strength_factor: f64,
    loops: i32,
    chance: i32,
) -> DecoHelper5050 {
    let oak = nigra_tree_deco(
        272,
        296,
        max_y,
        min_trunk,
        max_trunk,
        min_crown,
        max_crown,
        strength_factor,
        loops,
        chance,
    );
    let spruce = nigra_tree_deco(
        block_rules::wood_state(1, false),
        block_rules::wood_state(1, true),
        max_y,
        min_trunk,
        max_trunk,
        min_crown,
        max_crown,
        strength_factor,
        loops,
        chance,
    );
    DecoHelper5050::new(Box::new(oak), Box::new(spruce))
}

/// 极端丘陵各变体共用的装饰：橡木/云杉倒木 50/50 + 橡木/云杉灌木 50/50 + 巨石。
fn extreme_hills_common_decos() -> Vec<Box<dyn Deco>> {
    let oak_logs = DecoFallenTree {
        distribution: Distribution::new(100.0, 6.0, f(0.8)),
        log_condition_chance: 16,
        log_block: 272,
        leaves_block: 296,
        min_size: 4,
        max_size: 7,
        max_y: 75,
        ..DecoFallenTree::default()
    };
    let spruce_logs = DecoFallenTree {
        distribution: Distribution::new(100.0, 6.0, f(0.8)),
        log_condition_chance: 16,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        min_size: 4,
        max_size: 7,
        max_y: 75,
        ..DecoFallenTree::default()
    };
    let logs = DecoHelper5050::new(Box::new(oak_logs), Box::new(spruce_logs));

    let oak_shrub = DecoShrub {
        log_block: 272,
        leaves_block: 296,
        loop_multiplier: f(4.0),
        chance: 2,
        max_y: 110,
        ..DecoShrub::default()
    };
    let spruce_shrub = DecoShrub {
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        loop_multiplier: f(4.0),
        chance: 2,
        max_y: 110,
        ..DecoShrub::default()
    };
    let shrubs = DecoHelper5050::new(Box::new(oak_shrub), Box::new(spruce_shrub));

    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 12,
        max_y: 90,
        strength_factor: f(2.0),
        ..DecoBoulder::default()
    };

    vec![Box::new(logs), Box::new(shrubs), Box::new(boulder)]
}

/// 对应 `DecoCollectionExtremeHills` + `ExtremeHillsCommon`（biome 3）。
/// 黑松 50/50（maxY=85, trunk 12..18, crown 10..14, strengthFactor=3.0, chance=20）。
pub fn extreme_hills_decos() -> Vec<Box<dyn Deco>> {
    let tree = nigra_5050(85, 12, 18, 10, 14, 3.0, 1, 20);
    let mut decos = vec![Box::new(tree) as Box<dyn Deco>];
    decos.extend(extreme_hills_common_decos());
    decos
}

/// 对应 `VanillaBiomeDecos20`（biome 20 极端丘陵边缘）。
/// 单棵橡木黑松（maxY=100, trunk 18..27, crown 7..10, strengthFactor=4.0, chance=24）
/// + 云杉倒木 + 灌木 + 巨石 + 蘑菇 + 南瓜。
pub fn extreme_hills_edge_decos() -> Vec<Box<dyn Deco>> {
    let tree = nigra_tree_deco(272, 296, 100, 18, 27, 7, 10, 4.0, 1, 24);

    let fallen = DecoFallenTree {
        distribution: Distribution::new(100.0, 6.0, f(0.8)),
        log_condition_chance: 6,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let shrub = DecoShrub {
        max_y: 100,
        loop_multiplier: f(2.0),
        ..DecoShrub::default()
    };

    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 12,
        max_y: 95,
        strength_factor: f(2.0),
        ..DecoBoulder::default()
    };

    let mushrooms = DecoMushrooms {
        max_y: 90,
        ..DecoMushrooms::default()
    };

    let pumpkin = DecoPumpkin {
        max_y: 90,
        ..DecoPumpkin::default()
    };

    vec![
        Box::new(tree),
        Box::new(fallen),
        Box::new(shrub),
        Box::new(boulder),
        Box::new(mushrooms),
        Box::new(pumpkin),
    ]
}

/// 对应 `DecoCollectionExtremeHillsPlus` + `ExtremeHillsCommon`（biome 34）。
/// 两组黑松 50/50：(maxY=85, trunk 14..20, crown 10..14) + (maxY=95, trunk 10..14, crown 8..10)，
/// strengthFactor=4.0, chance=12。
pub fn extreme_hills_plus_decos() -> Vec<Box<dyn Deco>> {
    let tree1 = nigra_5050(85, 14, 20, 10, 14, 4.0, 1, 12);
    let tree2 = nigra_5050(95, 10, 14, 8, 10, 4.0, 1, 12);
    let mut decos = vec![
        Box::new(tree1) as Box<dyn Deco>,
        Box::new(tree2) as Box<dyn Deco>,
    ];
    decos.extend(extreme_hills_common_decos());
    decos
}

/// 对应 `DecoCollectionExtremeHillsM` + `ExtremeHillsCommon`（biome 131）。
/// 两组黑松 50/50：(maxY=85, trunk 10..18, crown 10..18) + (maxY=95, trunk 10..12, crown 10..12)，
/// 用 loops=4（非 strengthFactor）, chance=3。
pub fn extreme_hills_m_decos() -> Vec<Box<dyn Deco>> {
    let tree1 = nigra_5050(85, 10, 18, 10, 18, 0.0, 4, 3);
    let tree2 = nigra_5050(95, 10, 12, 10, 12, 0.0, 4, 3);
    let mut decos = vec![
        Box::new(tree1) as Box<dyn Deco>,
        Box::new(tree2) as Box<dyn Deco>,
    ];
    decos.extend(extreme_hills_common_decos());
    decos
}

/// 对应 `DecoCollectionExtremeHillsPlusM` + `ExtremeHillsCommon`（biome 162）。
/// 两组黑松 50/50：(maxY=85, trunk 14..20, crown 10..14) + (maxY=95, trunk 10..14, crown 8..10)，
/// 用 loops=3（非 strengthFactor）, chance=4。
pub fn extreme_hills_plus_m_decos() -> Vec<Box<dyn Deco>> {
    let tree1 = nigra_5050(85, 14, 20, 10, 14, 0.0, 3, 4);
    let tree2 = nigra_5050(95, 10, 14, 8, 10, 0.0, 3, 4);
    let mut decos = vec![
        Box::new(tree1) as Box<dyn Deco>,
        Box::new(tree2) as Box<dyn Deco>,
    ];
    decos.extend(extreme_hills_common_decos());
    decos
}

/// 对应 `DecoCollectionDesertRiver`：河边金合欢 + 灌木 + 仙人掌 + 甘蔗。
///
/// PHP 里这些 deco 都带 `checkRiver/minRiver` 过滤，只有靠近河道才会刷。
/// 目前 Rust 的 `Deco` trait 还没接 river 参数，所以暂时不过滤——
/// 这些装饰会按各自的高度/概率条件在整个 biome 里散布，不影响正确性，
/// 只是河边密度会比参考实现低一些。
fn desert_river_decos() -> Vec<Box<dyn Deco>> {
    let acacia_materials = crate::level::generator::realistic_terrain::tree::TreeMaterials::ACACIA;

    // 金合欢小树：trunk 2..4, crown 3..6。PHP 里同一棵树既单独放又塞进
    // DecoHelperThisOrThat，这里构造两次（DecoTree 含 Box<dyn Tree> 不能 Clone）。
    fn acacia_tree_deco() -> DecoTree {
        let mut tree = TreeAcaciaBucheri::default();
        tree.base.min_trunk_size = 2;
        tree.base.max_trunk_size = 4;
        tree.base.min_crown_size = 3;
        tree.base.max_crown_size = 6;
        DecoTree {
            loops: 1,
            max_y: 65,
            tree_condition: 0,
            ..DecoTree::from_tree(Box::new(tree))
        }
    }

    let acacia_shrub = DecoShrub {
        log_block: acacia_materials.log,
        leaves_block: acacia_materials.leaves,
        max_y: 65,
        loops: 1,
        chance: 1,
        ..DecoShrub::default()
    };

    let river_cactus = DecoCactus {
        max_y: 80,
        strength_factor: f(12.0),
        ..DecoCactus::default()
    };

    let reed = DecoReed {
        max_y: 68,
        loops: 3,
        ..DecoReed::default()
    };

    vec![
        Box::new(acacia_tree_deco()),
        Box::new(DecoHelperThisOrThat::new(
            4,
            1,
            Box::new(acacia_shrub),
            Box::new(acacia_tree_deco()),
        )),
        Box::new(river_cactus),
        Box::new(reed),
    ]
}

/// 对应 `DecoCollectionSavanna`：灌木 + 倒木 + 变量金合欢 + 巨石 + 双格高草。
pub fn savanna_decos() -> Vec<Box<dyn Deco>> {
    let acacia_materials = crate::level::generator::realistic_terrain::tree::TreeMaterials::ACACIA;
    let acacia_shrub = DecoShrub {
        log_block: acacia_materials.log,
        leaves_block: acacia_materials.leaves,
        max_y: 160,
        loop_multiplier: f(2.0),
        chance: 12,
        ..DecoShrub::default()
    };

    let fallen_tree = DecoFallenTree {
        loops: 1,
        distribution: Distribution::new(100.0, 6.0, 0.800000011920929),
        log_condition_chance: 36,
        log_block: acacia_materials.log,
        leaves_block: acacia_materials.leaves,
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    let boulder = DecoBoulder {
        boulder_block: 64,
        chance: 32,
        max_y: 95,
        ..DecoBoulder::default()
    };

    let double_grass = DecoDoubleGrass {
        max_y: 128,
        strength_factor: f(3.0),
        ..DecoDoubleGrass::default()
    };

    vec![
        Box::new(acacia_shrub),
        Box::new(fallen_tree),
        Box::new(DecoVariableTree::acacia()),
        Box::new(boulder),
        Box::new(double_grass),
    ]
}

/// 对应 `VanillaBiomeDecos36` / `VanillaBiomeDecos164`：稀树草原高原。
///
/// 36 和 164 的 initDecos 完全相同（PHP 里 164 多挂了 DesertRiver，36 没挂——
/// 但 36 的 COLLECTIONS 映射也只有 DesertRiver，所以两者实际等价）。
pub fn savanna_plateau_decos() -> Vec<Box<dyn Deco>> {
    let acacia_materials = crate::level::generator::realistic_terrain::tree::TreeMaterials::ACACIA;

    // 两颗巨石：一颗低海拔(maxY=80)，一颗高海拔(minY=110)。
    // PHP 里有个 bug：`$decoBoulder1->setMinY(110)` 写到了第一颗上，
    // 这里按意图修正——第二颗才是高海拔巨石。
    let boulder_low = DecoBoulder {
        boulder_block: 64,
        max_y: 80,
        chance: 24,
        ..DecoBoulder::default()
    };
    let boulder_high = DecoBoulder {
        boulder_block: 64,
        min_y: 110,
        chance: 24,
        ..DecoBoulder::default()
    };

    let acacia_shrub = DecoShrub {
        log_block: acacia_materials.log,
        leaves_block: acacia_materials.leaves,
        max_y: 160,
        loop_multiplier: f(3.0),
        chance: 9,
        ..DecoShrub::default()
    };

    // 大金合欢：trunk 12..16
    let mut acacia_tree = TreeAcaciaBucheri::default();
    acacia_tree.base.min_trunk_size = 12;
    acacia_tree.base.max_trunk_size = 16;
    let acacia_trees = DecoTree {
        strength_factor_for_loops: f(2.0),
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 12,
        max_y: 90,
        ..DecoTree::from_tree(Box::new(acacia_tree))
    };

    let cactus = DecoCactus {
        max_y: 160,
        loops: 60,
        chance: 8,
        ..DecoCactus::default()
    };

    let double_grass = DecoDoubleGrass {
        max_y: 128,
        strength_factor: f(3.0),
        ..DecoDoubleGrass::default()
    };

    let mut decos: Vec<Box<dyn Deco>> = desert_river_decos();
    decos.extend([
        Box::new(boulder_low) as Box<dyn Deco>,
        Box::new(boulder_high) as Box<dyn Deco>,
        Box::new(acacia_shrub) as Box<dyn Deco>,
        Box::new(acacia_trees) as Box<dyn Deco>,
        Box::new(DecoVariableTree::acacia()) as Box<dyn Deco>,
        Box::new(cactus) as Box<dyn Deco>,
        Box::new(double_grass) as Box<dyn Deco>,
    ]);
    decos
}

/// 对应 biome 163（Savanna M）：DesertRiver + Savanna。
pub fn savanna_m_decos() -> Vec<Box<dyn Deco>> {
    let mut decos = desert_river_decos();
    decos.extend(savanna_decos());
    decos
}

/// 对应 `DecoCollectionJungle`（biome 21/22/149：丛林/丛林丘陵/丛林 M）。
///
/// 树种：巨型丛林树（WorldGenMegaJungle）与红树林（RhizophoraMucronata）按
/// 3:1 切换，另加椰子树（CocosNucifera）。地面装饰有倒木、花、丛林仙人掌和巨石。
pub fn jungle_decos() -> Vec<Box<dyn Deco>> {
    // 巨型丛林树：treeType=1 走 vanilla WorldGen，自带尺寸逻辑。
    let mega_jungle = DecoTree {
        loops: 3,
        tree_type: 1,
        tree_condition: 4,
        tree_condition_chance: 3,
        max_y: 160,
        ..DecoTree::from_tree(Box::new(TreeMegaJungle::default()))
    };

    // 红树林：treeType=0，min/max trunk/crown 均为 0（PHP 里未设置），
    // 所以 do_generate 算出的 trunkSize/crownSize 都是 0——整棵树只有顶部树枝。
    let mangroves = DecoTree {
        loops: 3,
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 3,
        max_y: 160,
        ..DecoTree::from_tree(Box::new(TreeRhizophoraMucronata::default()))
    };

    // nextInt(3) != 0 时选 megaJungle，否则选 mangroves，实际 2:1。
    let tree_split = DecoHelperThisOrThat::new(
        3,
        1,
        Box::new(mega_jungle),
        Box::new(mangroves),
    );

    // 椰子树：trunk 7..9 / crown 6..8。
    let mut palm = TreeCocosNucifera::default();
    palm.base.min_trunk_size = 7;
    palm.base.max_trunk_size = 9;
    palm.base.min_crown_size = 6;
    palm.base.max_crown_size = 8;
    let palm_custom = DecoTree {
        loops: 1,
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 4,
        max_y: 160,
        ..DecoTree::from_tree(Box::new(palm))
    };

    // 倒木：丛林木，size 4..9。
    let fallen_tree = DecoFallenTree {
        loops: 1,
        distribution: Distribution::new(100.0, 5.0, f(0.800000011920929)),
        log_condition_chance: 3,
        log_block: block_rules::wood_state(3, false),
        leaves_block: block_rules::wood_state(3, true),
        min_size: 4,
        max_size: 9,
        ..DecoFallenTree::default()
    };

    // 花：仅蒲公英（flower id 6）。
    let flowers = DecoFlowersRtg {
        flowers: vec![6],
        chance: 4,
        max_y: 120,
        strength_factor: f(2.0),
        ..DecoFlowersRtg::default()
    };

    // 丛林仙人掌：沙/草/土上均可生长。
    let jungle_cacti = DecoJungleCacti {
        strength_factor: f(8.0),
        max_y: 120,
        sand_only: false,
        extra_height: 7,
    };

    // 巨石：768 = 苔石(48<<4)。
    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 16,
        max_y: 95,
        strength_factor: f(2.0),
        ..DecoBoulder::default()
    };

    vec![
        Box::new(tree_split),
        Box::new(palm_custom),
        Box::new(fallen_tree),
        Box::new(flowers),
        Box::new(jungle_cacti),
        Box::new(boulder),
    ]
}

/// 对应 `VanillaBiomeDecos23` / `VanillaBiomeDecos151`：丛林边缘 / 丛林边缘 M。
///
/// 只有一根倒木（丛林木，size 3..6），没有树。
pub fn jungle_edge_decos() -> Vec<Box<dyn Deco>> {
    let fallen_tree = DecoFallenTree {
        loops: 1,
        distribution: Distribution::new(100.0, 6.0, f(0.800000011920929)),
        log_condition_chance: 6,
        log_block: block_rules::wood_state(3, false),
        leaves_block: block_rules::wood_state(3, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };

    vec![Box::new(fallen_tree)]
}

/// 对应 PHP `TreeRTG::$ROOFED_FOREST_LIGHT_OBSTRUCTION_LIMIT`。
const ROOFED_FOREST_OBSTRUCTION: i32 = 6;

/// 黑森林用的深色橡木（variant 5）。
fn dark_oak_log() -> i32 {
    block_rules::wood_state(5, false)
}
fn dark_oak_leaves() -> i32 {
    block_rules::wood_state(5, true)
}

/// 对应 `VanillaBiomeDecos29`：黑森林（RoofedForest）。
///
/// 三种深色橡木树（红树林/吉贝/金合欢形）按 11:11:2 随机分布，
/// 地面有蘑菇、倒木、深色橡木/橡木灌木、巨石、枯草。
/// （蜘蛛网 DecoCobwebs 和大蘑菇 DecoWorldGen 暂未移植，跳过。）
pub fn roofed_forest_decos() -> Vec<Box<dyn Deco>> {
    // 红树林（深色橡木）：trunk 2..3 / crown 5..8
    let mut mangrove = TreeRhizophoraMucronata::default();
    mangrove.base.log_block = dark_oak_log();
    mangrove.base.leaves_block = dark_oak_leaves();
    mangrove.base.max_allowed_obstruction = ROOFED_FOREST_OBSTRUCTION;
    mangrove.base.min_trunk_size = 2;
    mangrove.base.max_trunk_size = 3;
    mangrove.base.min_crown_size = 5;
    mangrove.base.max_crown_size = 8;
    let mangrove_tree = DecoTree {
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 1,
        strength_factor_for_loops: f(18.0),
        max_y: 110,
        ..DecoTree::from_tree(Box::new(mangrove))
    };

    // 吉贝树（深色橡木）：trunk 2..3 / crown 5..8
    let mut ceiba = TreeCeibaPentandra::default();
    ceiba.base.log_block = dark_oak_log();
    ceiba.base.leaves_block = dark_oak_leaves();
    ceiba.base.max_allowed_obstruction = ROOFED_FOREST_OBSTRUCTION;
    ceiba.base.min_trunk_size = 2;
    ceiba.base.max_trunk_size = 3;
    ceiba.base.min_crown_size = 5;
    ceiba.base.max_crown_size = 8;
    let ceiba_tree = DecoTree {
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 1,
        strength_factor_for_loops: f(18.0),
        max_y: 110,
        ..DecoTree::from_tree(Box::new(ceiba))
    };

    // 金合欢形（深色橡木）：trunk 3..6 / crown 3..8
    let mut bucheri = TreeAcaciaBucheri::default();
    bucheri.base.log_block = dark_oak_log();
    bucheri.base.leaves_block = dark_oak_leaves();
    bucheri.base.trunk_size = 5;
    bucheri.base.min_trunk_size = 3;
    bucheri.base.max_trunk_size = 6;
    bucheri.base.min_crown_size = 3;
    bucheri.base.max_crown_size = 8;
    bucheri.base.max_allowed_obstruction = ROOFED_FOREST_OBSTRUCTION;
    let bucheri_trees = DecoTree {
        loops: 3,
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 1,
        ..DecoTree::from_tree(Box::new(bucheri))
    };

    // 11:11:2 随机选树
    let tree_split = DecoHelperRandomSplit::new(
        vec![
            Box::new(mangrove_tree) as Box<dyn Deco>,
            Box::new(ceiba_tree) as Box<dyn Deco>,
            Box::new(bucheri_trees) as Box<dyn Deco>,
        ],
        vec![11, 11, 2],
    );

    // 倒木：深色橡木，size 4..9
    let fallen_tree = DecoFallenTree {
        distribution: Distribution::new(80.0, 60.0, f(-15.0)),
        log_condition_chance: 16,
        log_block: dark_oak_log(),
        leaves_block: dark_oak_leaves(),
        min_size: 4,
        max_size: 9,
        ..DecoFallenTree::default()
    };

    // 深色橡木灌木 vs 橡木灌木 3:1
    let dark_oak_shrub = DecoShrub {
        log_block: dark_oak_log(),
        leaves_block: dark_oak_leaves(),
        max_y: 100,
        loop_multiplier: f(6.0),
        ..DecoShrub::default()
    };
    let oak_shrub = DecoShrub {
        log_block: 272,
        leaves_block: 296,
        max_y: 100,
        loop_multiplier: f(6.0),
        ..DecoShrub::default()
    };
    let shrub_split = DecoHelperThisOrThat::new(
        4,
        1,
        Box::new(dark_oak_shrub),
        Box::new(oak_shrub),
    );

    // 蘑菇
    let mushrooms = DecoMushrooms {
        chance: 4,
        max_y: 90,
        ..DecoMushrooms::default()
    };

    // 巨石
    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 16,
        max_y: 80,
        strength_factor: f(2.0),
        ..DecoBoulder::default()
    };

    // 枯草
    let dead_bush = DecoDeadBush {
        max_y: 100,
        chance: 2,
        strength_factor: f(2.0),
        ..DecoDeadBush::default()
    };

    // 蜘蛛网：挂在深色橡木原木旁（adjacentBlock=woodState(5,false)=2593），至少 2 邻。
    let cobwebs = DecoCobwebs {
        chance: 1,
        min_y: 63,
        max_y: 76,
        strength_factor: f(24.0),
        adjacent_block: dark_oak_log(),
        min_adjacents: 2,
        ..DecoCobwebs::default()
    };

    // 大蘑菇：随机棕/红，minY 63 / maxY 100 / chance 8。
    let big_shroom = DecoWorldGen {
        worldgen: Box::new(WorldGenBigMushroom::default()),
        min_y: 63,
        max_y: 100,
        chance: 8,
        ..DecoWorldGen::new(Box::new(WorldGenBigMushroom::default()))
    };

    vec![
        Box::new(mushrooms),
        Box::new(tree_split),
        Box::new(fallen_tree),
        Box::new(shrub_split),
        Box::new(boulder),
        Box::new(cobwebs),
        Box::new(dead_bush),
        Box::new(big_shroom),
    ]
}

/// 对应 `VanillaBiomeDecos157`：黑森林 M（RoofedForest M）。
///
/// 只有红树林（深色橡木），加上倒木、灌木、枯草、蘑菇、巨石。
pub fn roofed_forest_m_decos() -> Vec<Box<dyn Deco>> {
    // 巨石
    let boulder = DecoBoulder {
        boulder_block: 768,
        chance: 20,
        max_y: 80,
        strength_factor: f(2.0),
        ..DecoBoulder::default()
    };

    // 红树林（深色橡木）：trunk 3..4 / crown 7..12
    let mut mangrove = TreeRhizophoraMucronata::default();
    mangrove.base.log_block = dark_oak_log();
    mangrove.base.leaves_block = dark_oak_leaves();
    mangrove.base.min_trunk_size = 3;
    mangrove.base.max_trunk_size = 4;
    mangrove.base.min_crown_size = 7;
    mangrove.base.max_crown_size = 12;
    let trees = DecoTree {
        strength_factor_for_loops: f(24.0),
        distribution: Distribution::new(80.0, 60.0, f(-15.0)),
        tree_type: 0,
        tree_condition: 1,
        tree_condition_noise: f(0.0),
        tree_condition_chance: 1,
        max_y: 120,
        ..DecoTree::from_tree(Box::new(mangrove))
    };

    // 倒木
    let fallen_tree = DecoFallenTree {
        distribution: Distribution::new(80.0, 60.0, f(-15.0)),
        log_condition_chance: 16,
        log_block: dark_oak_log(),
        leaves_block: dark_oak_leaves(),
        min_size: 4,
        max_size: 9,
        ..DecoFallenTree::default()
    };

    // 灌木（默认橡木）
    let shrub = DecoShrub {
        max_y: 110,
        loop_multiplier: f(1.0),
        ..DecoShrub::default()
    };

    // 枯草
    let dead_bush = DecoDeadBush {
        max_y: 128,
        chance: 16,
        strength_factor: f(1.0),
        ..DecoDeadBush::default()
    };

    // 蘑菇
    let mushrooms = DecoMushrooms {
        max_y: 90,
        ..DecoMushrooms::default()
    };

    vec![
        Box::new(boulder),
        Box::new(trees),
        Box::new(fallen_tree),
        Box::new(shrub),
        Box::new(dead_bush),
        Box::new(mushrooms),
    ]
}

/// 对应 `DecoCollectionMesa`：平顶山家族共用的地面装饰。
///
/// 巨石（圆石）+ 灌木 + 枯草 + 仙人掌（只在沙地上长）。
fn mesa_ground_decos() -> Vec<Box<dyn Deco>> {
    let boulder = DecoBoulder {
        boulder_block: 64,
        max_y: 83,
        ..DecoBoulder::default()
    };
    let shrub = DecoShrub {
        loops: 2,
        chance: 4,
        max_y: 90,
        ..DecoShrub::default()
    };
    let dead_bush = DecoDeadBush {
        max_y: 100,
        loops: 3,
        ..DecoDeadBush::default()
    };
    let cactus = DecoCactus {
        soil_block: 192 | 1,
        loops: 10,
        max_y: 100,
        ..DecoCactus::default()
    };
    vec![
        Box::new(boulder),
        Box::new(shrub),
        Box::new(dead_bush),
        Box::new(cactus),
    ]
}

/// 对应 biome 37/39/165/167（Mesa/MesaPlateau/MesaBryce/MesaPlateauM）：
/// DesertRiver + Mesa 地面装饰，没有树。
pub fn mesa_decos() -> Vec<Box<dyn Deco>> {
    let mut decos = desert_river_decos();
    decos.extend(mesa_ground_decos());
    decos
}

/// 对应 biome 38/166（MesaPlateauF / MesaPlateauFM）：
/// DesertRiver + Mesa 地面装饰 + 高原顶上的小橡树。
pub fn mesa_plateau_decos() -> Vec<Box<dyn Deco>> {
    let mut decos = desert_river_decos();
    decos.extend(mesa_ground_decos());

    // 38 的 loops=24，166 的 loops=16；这里取中间值，因为两个 biome 共用。
    let tree = DecoTree {
        loops: 20,
        tree_type: 1,
        distribution: Distribution::new(24.0, 1.0, f(0.0)),
        tree_condition_chance: 1,
        tree_condition_float: f(4.0),
        tree_condition_noise: f(0.0),
        min_y: 74,
        ..DecoTree::from_tree(Box::new(VanillaOakTree::default()))
    };
    decos.push(Box::new(tree));
    decos
}

/// 冰刺树工厂：根据树冠范围构造一棵 IceSpike 的 DecoTree。
fn ice_spike_tree(min_crown: i32, max_crown: i32, noise_min: f64, noise_max: f64) -> DecoTree {
    let mut spike = IceSpikeRTG::default();
    spike.base.min_trunk_size = 1;
    spike.base.max_trunk_size = 1;
    spike.base.min_crown_size = min_crown;
    spike.base.max_crown_size = max_crown;
    DecoTree {
        strength_factor_for_loops: f(1.0),
        tree_type: 0,
        distribution: Distribution::new(100.0, 6.0, f(0.800000011920929)),
        tree_condition: 3,
        tree_condition_noise: f(noise_min),
        tree_condition_noise2: f(noise_max),
        tree_condition_chance: 2,
        max_y: 85,
        ..DecoTree::from_tree(Box::new(spike))
    }
}

/// 对应 `DecoCollectionIceTrees`：冰原家族（12/13/140）。
///
/// 三档冰刺树（高/矮/矮）按噪声分布，加上随机冰刺灌木和冰灌木。
pub fn ice_decos() -> Vec<Box<dyn Deco>> {
    // 高冰刺：crown 10..24，噪声 -1..3
    let tall = DecoHelper5050::new(
        Box::new(ice_spike_tree(10, 24, -1.0, 3.0)),
        Box::new(ice_spike_tree(10, 24, -1.0, 3.0)),
    );

    // 矮冰刺：crown 5..13，噪声 -3..-1
    let short1 = DecoHelper5050::new(
        Box::new(ice_spike_tree(5, 13, -3.0, -1.0)),
        Box::new(ice_spike_tree(5, 13, -3.0, -1.0)),
    );

    // 矮冰刺：crown 5..13，噪声 3..5
    let short2 = DecoHelper5050::new(
        Box::new(ice_spike_tree(5, 13, 3.0, 5.0)),
        Box::new(ice_spike_tree(5, 13, 3.0, 5.0)),
    );

    // 随机冰刺灌木：crown 5..11，无树叶
    let mut bush = IceSpikeRTG::default();
    bush.base.min_trunk_size = 1;
    bush.base.max_trunk_size = 1;
    bush.base.min_crown_size = 5;
    bush.base.max_crown_size = 11;
    let pungens = DecoTree {
        strength_factor_for_loops: f(1.0),
        tree_type: 0,
        tree_condition: 4,
        tree_condition_chance: 8,
        max_y: 100,
        ..DecoTree::from_tree(Box::new(bush))
    };

    // 冰灌木：log=2784（浮冰），leaves=0
    let shrub = DecoShrub {
        log_block: 2784,
        leaves_block: 0,
        max_y: 140,
        loop_multiplier: f(4.0),
        chance: 3,
        ..DecoShrub::default()
    };

    vec![
        Box::new(tall),
        Box::new(short1),
        Box::new(short2),
        Box::new(pungens),
        Box::new(shrub),
    ]
}

/// 对应 `VanillaBiomeDecos16`：沙滩（Beach）。
///
/// 只在沙地上长椰子树。
pub fn beach_decos() -> Vec<Box<dyn Deco>> {
    let mut palm = TreeCocosNucifera::default();
    palm.base.min_trunk_size = 7;
    palm.base.max_trunk_size = 9;
    palm.base.min_crown_size = 6;
    palm.base.max_crown_size = 8;
    // 只允许沙地（block 12, state 192）。
    palm.base.valid_ground_blocks = vec![192];
    let palm_trees = DecoTree {
        tree_type: 0,
        tree_condition: 1,
        tree_condition_noise: f(-0.20000000298023224),
        tree_condition_chance: 12,
        max_y: 68,
        ..DecoTree::from_tree(Box::new(palm))
    };
    vec![Box::new(palm_trees)]
}

/// 对应 `VanillaBiomeDecos26`：冷沙滩（Cold Beach）。
///
/// 只有圆石巨石。
pub fn cold_beach_decos() -> Vec<Box<dyn Deco>> {
    let boulder = DecoBoulder {
        boulder_block: 64,
        chance: 16,
        max_y: 95,
        strength_factor: f(3.0),
        ..DecoBoulder::default()
    };
    vec![Box::new(boulder)]
}

/// 对应 `VanillaBiomeDecos132`：繁花森林（Flower Forest）。
///
/// 灌木 + 大量花 + 橡树/云杉变量树 + 橡木/云杉倒木。
pub fn flower_forest_decos() -> Vec<Box<dyn Deco>> {
    let shrub = DecoShrub {
        max_y: 110,
        loop_multiplier: f(4.0),
        chance: 3,
        ..DecoShrub::default()
    };

    let flowers1 = DecoFlowersRtg {
        flowers: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 0],
        strength_factor: f(12.0),
        height_type: 1,
        ..DecoFlowersRtg::default()
    };

    let flowers2 = DecoFlowersRtg {
        flowers: vec![0, 1, 4, 5],
        chance: 8,
        height_type: 1,
        ..DecoFlowersRtg::default()
    };

    let mut trees = DecoVariableMaterialTree::new(TreeMaterialsChooser::in_oak_forest());
    trees.base.strength_factor_for_loops = f(0.5);
    trees.base.tree_condition = 0;
    trees.base.tree_condition_noise = f(0.0);
    trees.base.tree_condition_noise2 = f(1.0);
    trees.base.tree_condition_chance = 1;
    trees.base.max_y = 120;

    let fallen_oak = DecoFallenTree {
        log_condition: 4,
        log_condition_chance: 8,
        max_y: 100,
        log_block: 272,
        leaves_block: 296,
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };
    let fallen_spruce = DecoFallenTree {
        log_condition: 4,
        log_condition_chance: 8,
        max_y: 100,
        log_block: block_rules::wood_state(1, false),
        leaves_block: block_rules::wood_state(1, true),
        min_size: 3,
        max_size: 6,
        ..DecoFallenTree::default()
    };
    let fallen_tree = DecoHelper5050::new(Box::new(fallen_oak), Box::new(fallen_spruce));

    vec![
        Box::new(shrub),
        Box::new(flowers1),
        Box::new(flowers2),
        Box::new(trees),
        Box::new(fallen_tree),
    ]
}

/// 对应 `DecoCollectionOcean`（biome 0/24：海洋 / 深海）。
///
/// 两种海底巨石（苔石 768 / 海晶石 2688）按 8:8 随机分布，加上湿海绵团。
/// 巨石 `heightType=0` 表示在 minY..maxY 间随机取高度（不贴地表）。
pub fn ocean_decos() -> Vec<Box<dyn Deco>> {
    let moss_boulder = DecoBoulder {
        boulder_block: 768,
        chance: 6,
        min_y: 22,
        max_y: 58,
        height_type: 0,
        strength_factor: f(3.0),
        ..DecoBoulder::default()
    };
    let prismarine_boulder = DecoBoulder {
        boulder_block: 2688,
        chance: 6,
        min_y: 22,
        max_y: 58,
        height_type: 0,
        strength_factor: f(3.0),
        ..DecoBoulder::default()
    };
    let boulders = DecoHelperRandomSplit::new(
        vec![
            Box::new(moss_boulder) as Box<dyn Deco>,
            Box::new(prismarine_boulder) as Box<dyn Deco>,
        ],
        vec![8, 8],
    );

    let sponge = DecoSponge {
        sponge_block: (19 << 4) | 1, // 湿海绵
        chance: 500,
        min_y: 22,
        max_y: 39,
        height_type: 0,
        strength_factor: f(8.0),
        ..DecoSponge::default()
    };

    vec![Box::new(boulders), Box::new(sponge)]
}

/// 对应 `VanillaBiomeDecorationProfile` 中 mushroom_island（biome 14/15）。
///
/// profile：`trees=-100, flowers=-100, grass=-100, mushrooms=1, bigMushrooms=1`。
/// 小蘑菇用 `DecoMushrooms`；大蘑菇用 `DecoWorldGen(WorldGenBigMushroom)`，
/// loops=0 即每区块尝试 1 次。
pub fn mushroom_island_decos() -> Vec<Box<dyn Deco>> {
    let mushrooms = DecoMushrooms {
        max_y: 128,
        ..DecoMushrooms::default()
    };
    let big_shroom = DecoWorldGen {
        worldgen: Box::new(WorldGenBigMushroom::default()),
        loops: 0,
        max_y: 128,
        ..DecoWorldGen::new(Box::new(WorldGenBigMushroom::default()))
    };
    vec![Box::new(mushrooms), Box::new(big_shroom)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plains_has_one_composite_deco() {
        let decos = decos_for_biome(1);
        // 树随机分配 + 甘蔗 + 南瓜 + 小麦 = 4
        assert_eq!(decos.len(), 4);
        // 配置必须完整，否则构造时就会 panic
        assert!(decos.iter().all(|deco| deco.properly_defined()));
    }

    #[test]
    fn forest_has_complete_decoration_chain() {
        for biome in [4, 18] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 5);
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn desert_has_cactus_and_dead_bush() {
        for biome in [2, 17, 130] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 2);
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn taiga_has_full_decoration_stack() {
        for biome in [5, 19, 30, 31] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 8, "biome {biome} 应有 8 个装饰组件");
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn swamp_has_willow_dead_pine_shrub_fallen_pumpkin() {
        for biome in [6, 134] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 5, "biome {biome} 应有 5 个装饰组件");
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn birch_forest_has_trees_logs_shrubs_flowers() {
        for biome in [27, 28] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 4, "biome {biome} 应有 4 个装饰组件");
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn birch_forest_m_has_same_components_as_regular() {
        for biome in [155, 156] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 4, "biome {biome} 应有 4 个装饰组件");
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn mega_taiga_has_boulder_three_trees_fallen_shrub_mushroom_pumpkin() {
        for biome in [32, 33] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 6, "biome {biome} 应有 6 个装饰组件");
            assert!(decos.iter().all(|deco| deco.properly_defined()));
        }
    }

    #[test]
    fn extreme_hills_family_has_proper_deco_chains() {
        // biome 3: 1 树(5050) + 3 common = 4
        assert_eq!(decos_for_biome(3).len(), 4);
        // biome 20: 树+倒木+灌木+巨石+蘑菇+南瓜 = 6
        assert_eq!(decos_for_biome(20).len(), 6);
        // biome 34: 2 树(5050) + 3 common = 5
        assert_eq!(decos_for_biome(34).len(), 5);
        // biome 131: 2 树(5050) + 3 common = 5
        assert_eq!(decos_for_biome(131).len(), 5);
        // biome 162: 2 树(5050) + 3 common = 5
        assert_eq!(decos_for_biome(162).len(), 5);
        for biome in [3, 20, 34, 131, 162] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn jungle_family_has_expected_deco_chains() {
        // biome 21/22/149 (Jungle/Hills/M)：树切换 + 椰子树 + 倒木 + 花 + 丛林仙人掌 + 巨石 = 6
        for biome in [21, 22, 149] {
            assert_eq!(decos_for_biome(biome).len(), 6, "biome {biome}");
        }
        // biome 23/151 (Jungle Edge/M)：仅倒木 = 1
        for biome in [23, 151] {
            assert_eq!(decos_for_biome(biome).len(), 1, "biome {biome}");
        }
        for biome in [21, 22, 23, 149, 151] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn roofed_forest_family_has_expected_deco_chains() {
        // biome 29 (RoofedForest)：蘑菇 + 树随机 + 倒木 + 灌木切换 + 巨石 + 蜘蛛网 + 枯草 + 大蘑菇 = 8
        assert_eq!(decos_for_biome(29).len(), 8);
        // biome 157 (RoofedForest M)：巨石 + 树 + 倒木 + 灌木 + 枯草 + 蘑菇 = 6
        assert_eq!(decos_for_biome(157).len(), 6);
        for biome in [29, 157] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn mesa_family_has_expected_deco_chains() {
        // biome 37/39/165/167：DesertRiver(4) + Mesa 地面(4) = 8
        for biome in [37, 39, 165, 167] {
            assert_eq!(decos_for_biome(biome).len(), 8, "biome {biome}");
        }
        // biome 38/166：DesertRiver(4) + Mesa 地面(4) + 橡树(1) = 9
        for biome in [38, 166] {
            assert_eq!(decos_for_biome(biome).len(), 9, "biome {biome}");
        }
        for biome in [37, 38, 39, 165, 166, 167] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn ice_family_has_expected_deco_chains() {
        // biome 12/13/140：高冰刺 + 矮冰刺×2 + 冰刺灌木 + 冰灌木 = 5
        for biome in [12, 13, 140] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 5, "biome {biome}");
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn beach_flower_forest_has_expected_deco_chains() {
        // biome 16 (Beach)：椰子树 = 1
        assert_eq!(decos_for_biome(16).len(), 1);
        // biome 26 (Cold Beach)：巨石 = 1
        assert_eq!(decos_for_biome(26).len(), 1);
        // biome 132 (Flower Forest)：灌木 + 花×2 + 变量树 + 倒木 = 5
        assert_eq!(decos_for_biome(132).len(), 5);
        for biome in [16, 26, 132] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn savanna_family_has_expected_deco_chains() {
        // biome 35 (Savanna): 灌木 + 倒木 + 变量金合欢 + 巨石 + 双格高草 = 5
        assert_eq!(decos_for_biome(35).len(), 5);
        // biome 36/164 (Savanna Plateau/M): DesertRiver(4) + 2 巨石 + 灌木 + 金合欢 + 变量金合欢 + 仙人掌 + 双格高草 = 11
        for biome in [36, 164] {
            assert_eq!(decos_for_biome(biome).len(), 11, "biome {biome}");
        }
        // biome 163 (Savanna M): DesertRiver(4) + Savanna(5) = 9
        assert_eq!(decos_for_biome(163).len(), 9);
        for biome in [35, 36, 163, 164] {
            let decos = decos_for_biome(biome);
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn mushroom_island_has_mushrooms_and_big_mushroom() {
        // biome 14/15 (Mushroom Island/Shore)：蘑菇 + 大蘑菇 = 2
        for biome in [14, 15] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 2, "biome {biome} 应有 2 个装饰组件");
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
    }

    #[test]
    fn ocean_family_has_boulders_and_sponge() {
        // biome 0 (Ocean) / 24 (Deep Ocean)：巨石随机分配 + 海绵 = 2
        for biome in [0, 24] {
            let decos = decos_for_biome(biome);
            assert_eq!(decos.len(), 2, "biome {biome} 应有 2 个装饰组件");
            assert!(
                decos.iter().all(|deco| deco.properly_defined()),
                "biome {biome} 有残缺装饰"
            );
        }
        // biome 7/10/11/25 (River/FrozenOcean/FrozenRiver/StoneBeach)：无装饰
        for biome in [7, 10, 11, 25] {
            assert!(decos_for_biome(biome).is_empty());
        }
    }
}
