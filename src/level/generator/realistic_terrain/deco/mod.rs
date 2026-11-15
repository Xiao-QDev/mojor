//! 对应 `rtg/deco/`：区块装饰阶段。
//!
//! RTG 在这一层决定"这个区块里长几棵树、长多大、种什么"。编排逻辑分三层：
//!
//! - [`DecoBase`] 级别的公共助手（偏移坐标、随机范围、树叶属性微调）
//! - [`DecoTree`] 单种树的散布规则（噪声门限 + 密度预算 + 高度决定树种档位）
//! - `biome::*` 每个 biome 的装饰清单（还没搬完，见 [`plains`]）
//!
//! 目前只接了 Plains / Forest 这一类"以橡木为主"的 biome，其他 biome 暂时没有
//! 装饰——先让树在真机出现，剩下的按 biome 往上叠。

pub mod boulder;
pub mod chunk_info;
pub mod cactus;
pub mod cobwebs;
pub mod collections;
pub mod crop;
pub mod dead_bush;
pub mod distribution;
pub mod double_grass;
pub mod fallen_tree;
pub mod flowers;
pub mod helper;
pub mod jungle_cacti;
pub mod mushrooms;
pub mod ore;
pub mod pumpkin;
pub mod reed;
pub mod shrub;
pub mod sponge;
pub mod tree;
pub mod vanilla_decorator;
pub mod variable_tree;
pub mod world_gen;

pub use boulder::DecoBoulder;
pub use cactus::DecoCactus;
pub use chunk_info::ChunkInfo;
pub use cobwebs::DecoCobwebs;
pub use crop::DecoCrop;
pub use dead_bush::DecoDeadBush;
pub use distribution::Distribution;
pub use double_grass::DecoDoubleGrass;
pub use fallen_tree::{DecoFallenTree, DecoVariableFallenTree, FallenWoodland};
pub use flowers::DecoFlowersRtg;
pub use helper::{DecoHelper5050, DecoHelperRandomSplit, DecoHelperThisOrThat};
pub use jungle_cacti::DecoJungleCacti;
pub use mushrooms::DecoMushrooms;
pub use ore::{OreDecoration, OreSettings};
pub use pumpkin::DecoPumpkin;
pub use reed::DecoReed;
pub use shrub::DecoShrub;
pub use sponge::DecoSponge;
pub use tree::DecoTree;
pub use vanilla_decorator::{
    generate_vanilla_decoration, profile_for_biome, VanillaProfile,
};
pub use variable_tree::{DecoVariableMaterialTree, DecoVariableTree, TreeMaterialsChooser};
pub use world_gen::DecoWorldGen;

use crate::level::generator::population::Pos;
use crate::level::generator::population::block_rules;
use crate::level::generator::realistic_terrain::math::JavaRandom;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::world::GenWorld;

/// 对应 `rtg/deco/DecoBase.php` 的子类契约。
///
/// PHP 里 `generate($biome, $rtgWorld, $rand, $chunkPos, $river, $hasVillage, $chunkInfo)`
/// 是统一入口；`$biome` 只用来读 `treeDensityMultiplier()`，`$river` 目前没有装饰
/// 用到，所以这里把这两项收进参数里而不是造一个上下文对象。
pub trait Deco {
    /// 参数多是因为照着 PHP 的七参签名搬，拆成结构体反而和参考实现不好对位。
    #[allow(clippy::too_many_arguments)]
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
    );

    /// 对应 `properlyDefined()`：配置残缺时 PHP 会抛异常，这里只用来做断言。
    fn properly_defined(&self) -> bool {
        true
    }
}

/// 对应 `DecoBase::getOffsetPos`：区块编号 → 区块中心的水平坐标。
pub fn offset_pos(chunk_x: i32, chunk_z: i32) -> Pos {
    Pos::new(chunk_x * 16 + 8, 0, chunk_z * 16 + 8)
}

/// 对应 `DecoBase::getRangedRandom`。
pub fn get_ranged_random(rand: &mut JavaRandom, min: i32, max: i32) -> i32 {
    min + rand.next_int_bound(max - min + 1)
}

/// 对应 `DecoBase::tweakTreeLeaves`：把树叶的 `check_decay`/`decayable` 位拨到
/// 生成期需要的值（不然树叶会立刻消失）。
pub fn tweak_tree_leaves(leaves_block: i32, check_decay: bool, decayable: bool) -> i32 {
    if !matches!(leaves_block >> 4, 18 | 161) {
        return leaves_block;
    }
    let decay = i32::from(check_decay);
    let decaying = i32::from(decayable);
    block_rules::with_property(
        block_rules::with_property(leaves_block, "check_decay", decay).unwrap_or(leaves_block),
        "decayable",
        decaying,
    )
    .unwrap_or(leaves_block)
}

/// 对应 `BufferedWorld::getHeight`：从顶部往下第一个有遮光的方块，返回 y+1。
///
/// 这里返回的是"地面之上那一格"，`DecoTree` 直接拿来做 `pos.up(y)`。
pub fn surface_height(world: &GenWorld, x: i32, z: i32) -> i32 {
    let mut y = crate::level::format::chunk::CHUNK_HEIGHT as i32 - 1;
    while y >= 0 {
        if block_rules::light_opacity(world.block_state(x, y, z)) > 0 {
            break;
        }
        y -= 1;
    }
    y + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn world() -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> = Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        GenWorld::new(chunks)
    }

    #[test]
    fn offset_pos_is_chunk_center() {
        assert_eq!(offset_pos(0, 0), Pos::new(8, 0, 8));
        assert_eq!(offset_pos(-1, 2), Pos::new(-8, 0, 40));
    }

    #[test]
    fn surface_height_finds_first_covered_block() {
        let mut world = world();
        assert_eq!(surface_height(&world, 3, 3), 0, "空区块高度为 0");
        world.set_block_state(3, 63, 3, 32); // 草
        world.set_block_state(3, 64, 3, 18 << 4); // 树叶
        // 树叶遮光 > 0，所以高度是树叶上方
        assert_eq!(surface_height(&world, 3, 3), 65);
    }

    #[test]
    fn tweak_tree_leaves_clears_decay_bits() {
        // 296 = 18<<4 | 8（check_decay）
        let tweaked = tweak_tree_leaves(296, false, true);
        assert_eq!(tweaked >> 4, 18);
        assert_eq!(tweaked & 8, 0, "check_decay 应被清掉");
        assert_eq!(tweaked & 4, 0, "decayable=true 时第 2 位是 0");
    }
}
