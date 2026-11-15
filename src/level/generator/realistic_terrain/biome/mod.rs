//! 对应 `rtg/biome/`。
//!
//! 分层 biome 网格：`GenLayer` 基类 + 20 个具体层，由
//! `initialize_all_biome_generators` 串成一条链，末端输出每格 biome id。
//!
//! | PHP | Rust |
//! |---|---|
//! | `grid/GenLayer.php` | [`gen_layer`] |
//! | `grid/GenLayer*.php` | [`layers`] |
//! | `grid/GenLayer.php::initializeAllBiomeGenerators` | [`layers::initialize_all_biome_generators`] |
//! | `BiomeProvider.php` | [`provider`] |
//! | `Biome.php` | [`table`] |
//! | `BiomeAnalyzer.php` | [`analyzer`] |
//! | `IntCache.php` | 不需要，见 [`gen_layer`] 顶部说明 |
//! | `GenLayerCache.php` | 不需要，[`provider`] 已按 64×64 分块缓存 |

pub mod analyzer;
pub mod gen_layer;
pub mod layers;
pub mod provider;
pub mod table;

pub use provider::BiomeProvider;
