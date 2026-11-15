//! 对应 `rtg/noise/`。
//!
//! | PHP | Rust |
//! |---|---|
//! | `OpenSimplexNoise.php` | [`open_simplex`] |
//! | `SpacedCellularNoise.php` | [`spaced_cellular`] |
//! | `VoronoiResult.php` | [`voronoi`] |
//! | `VanillaBiomeNoise.php` / `VanillaTemperatureNoise.php` | [`vanilla_simplex`] |
//! | `NoiseTables.php` | `super::noise_tables`（机械生成） |
//! | `SimplexData2D.php` | 不需要：Rust 直接返回 `[f64; 2]` |

pub mod open_simplex;
pub mod spaced_cellular;
pub mod vanilla_simplex;
pub mod voronoi;

pub use open_simplex::OpenSimplexNoise;
pub use spaced_cellular::SpacedCellularNoise;
pub use vanilla_simplex::VanillaSimplex;
pub use voronoi::VoronoiResult;
