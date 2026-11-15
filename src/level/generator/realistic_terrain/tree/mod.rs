//! 对应 `rtg/tree/`：RTG 的树。
//!
//! 和 vanilla 的"粗树干 + 球形树冠"不同，RTG 的树是让树枝沿方向向量爬行、
//! 走到阈值就分叉，所以每棵树形状都不一样，森林看起来更自然。
//!
//! 目录：
//! - `branch`   —— 方向向量、带小数位置、爬行中的树枝
//! - `skylight` —— 种树前的遮光预算（避免森林挤成一坨黑）
//! - `base`     —— `TreeRTG` 公共配置与放置原语
//! - `betula`   —— 白桦（第一个移植的树种）

pub mod base;
pub mod betula;
pub mod betula_nigra;
pub mod betula_utilis;
pub mod acacia_bucheri;
pub mod branch;
pub mod ceiba_pentandra;
pub mod cocos_nucifera;
pub mod cupressus_sempervirens;
pub mod density;
pub mod ice_spike;
pub mod mega_jungle;
pub mod picea_pungens;
pub mod picea_sitchensis;
pub mod pinus_nigra;
pub mod pinus_ponderosa;
pub mod quercus_falcata;
pub mod quercus_nigra;
pub mod quercus_robur;
pub mod rhizophora_mucronata;
pub mod salix_myrtilloides;
pub mod skylight;
pub mod vachellia_tortilis;
pub mod vanilla;
pub mod vanilla_trees;

pub use base::{Tree, TreeRTG};
pub use acacia_bucheri::TreeAcaciaBucheri;
pub use betula::TreeBetulaPendula;
pub use betula_nigra::TreeBetulaNigra;
pub use betula_utilis::TreeBetulaUtilis;
pub use branch::{BranchVector, FractionalPos, TreeBranch};
pub use ceiba_pentandra::TreeCeibaPentandra;
pub use cocos_nucifera::TreeCocosNucifera;
pub use cupressus_sempervirens::TreeCupressusSempervirens;
pub use density::TreeDensityLimiter;
pub use ice_spike::IceSpikeRTG;
pub use mega_jungle::TreeMegaJungle;
pub use picea_pungens::TreePiceaPungens;
pub use picea_sitchensis::TreePiceaSitchensis;
pub use pinus_nigra::TreePinusNigra;
pub use pinus_ponderosa::TreePinusPonderosa;
pub use quercus_falcata::TreeQuercusFalcata;
pub use quercus_nigra::TreeQuercusNigra;
pub use quercus_robur::TreeQuercusRobur;
pub use rhizophora_mucronata::TreeRhizophoraMucronata;
pub use salix_myrtilloides::TreeSalixMyrtilloides;
pub use vachellia_tortilis::{TreeVachelliaTortilis, TreeVachelliaTortilisMega};
pub use skylight::{SkylightTracker, Spotlight};
pub use vanilla::{VanillaOakTree, WorldGen, WorldGenBigMushroom};
pub use vanilla_trees::{generate_vanilla_named_tree, VanillaTreeKind};

use crate::level::generator::population::Pos;

/// 对应 PHP 的 `F::f($value)`：先转 float32 再转回 f64。
///
/// RTG 的每个 `F::f(...)` 都是一次真实的精度截断（`unpack('f', pack('f', $v))`），
/// 树枝的落点靠它才能和参考实现逐格一致，不能省。
#[inline]
pub fn f(value: f64) -> f64 {
    value as f32 as f64
}

/// 对应 `rtg/tree/TreeMaterials.php`。
///
/// `log`/`leaves`/`branches` 都是完整 state（`id << 4 | meta`），
/// 其中 `branches` 是原木加"树皮全覆盖"位（12/13/14），用来画树枝更自然。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeMaterials {
    pub name: &'static str,
    pub log: i32,
    pub leaves: i32,
    pub branches: i32,
}

impl TreeMaterials {
    pub const SPRUCE: Self = Self {
        name: "Spruce",
        log: (17 << 4) | 1,
        leaves: (18 << 4) | 1,
        branches: (17 << 4) | 13,
    };
    pub const OAK: Self = Self {
        name: "Oak",
        log: 17 << 4,
        leaves: 18 << 4,
        branches: (17 << 4) | 12,
    };
    pub const BIRCH: Self = Self {
        name: "Birch",
        log: (17 << 4) | 2,
        leaves: (18 << 4) | 2,
        branches: (17 << 4) | 14,
    };
    pub const ACACIA: Self = Self {
        name: "Acacia",
        log: 162 << 4,
        leaves: 161 << 4,
        branches: (162 << 4) | 12,
    };

    /// 对应 `TreeMaterialsPicker::forNoise`：噪声决定这片林子偏针叶还是偏白桦。
    pub fn for_noise(noise: f64) -> Self {
        if noise <= -1.0 {
            Self::SPRUCE
        } else if noise < 1.0 {
            Self::OAK
        } else {
            Self::BIRCH
        }
    }
}

/// 对应 `rtg/tree/FoliageCoordinates.php`：一片树叶 + 它挂靠的树干高度。
#[derive(Clone, Copy, Debug)]
pub struct FoliageCoordinates {
    pub pos: Pos,
    /// 对应 `func_177999_q()`：树枝从树干哪个高度分出去
    pub height_base: i32,
}
