//! 忠实还原 MCPE 0.14.3 原版（vanilla）世界生成。
//!
//! - `terrain`：密度网格 + 三线性插值 + 表面层
//! - `genlayer`：GenLayer biome 链
//! - `mcpe_noise`：MCPE 的 mt19937 / Perlin 噪声栈
//! - `populator`：洞穴、矿脉、植被、树木等装饰阶段
//!
//! 共享基础设施（方块 ID、biome 表、噪声原语、随机数、`GenWorld`）
//! 留在上一层 `super`，不属于任何单一生成器。

pub mod caves;
pub mod decorate;
pub mod genlayer;
pub mod mcpe_noise;
pub mod populator;
pub mod surface;
pub mod terrain;

pub use terrain::TerrainGenerator;
