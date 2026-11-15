//! RTG（Realistic Terrain Generation）移植。
//!
//! 源工程：`example/level/generator/rtg/`（PHP，GPL-3.0，见 `LICENSE-RTG.txt`）。
//! 目标：逐模块搬到 Rust，每一层都用 PHP 原实现导出的黄金值做单测比对。
//!
//! 移植顺序（自底向上，下层未完成上层无法验证）：
//!
//! 1. `math`      —— JavaRandom 48 位 LCG、float32 截断、Java 取整
//! 2. `noise`     —— OpenSimplex、SpacedCellular、查表常量
//! 3. `biome`     —— BiomeProvider + 40 个 GenLayer
//! 4. `terrain`   —— BiomeProfiles、TerrainBase、高度效果链
//! 5. `landscape` —— LandscapeGenerator（21×21 邻域加权）+ ChunkPrimer
//! 6. `surface`   —— 59 个 biome 地表构造器
//! 7. `terrain_generator` —— 区块总装（biome 抖动 → 地表 → 河流绿洲 → 基岩 → carver）
//! 8. `carver`    —— 洞穴、峡谷（已接入）
//! 9. 其余        —— deco / tree / feature / structure
//!
//! 注意：RTG 是 Java 精度敏感代码，浮点必须按 PHP 里的 `F::f()` 位置做 float32 落地，
//! 否则高度场会整体偏移。

pub mod adapter;
pub mod biome;
pub mod carver;
pub mod chunk_primer;
pub mod deco;
pub mod landscape;
pub mod math;
pub mod native_block_mapper;
pub mod noise;
pub mod noise_tables;
pub mod populate;
pub mod structure;
pub mod surface;
pub mod terrain;
pub mod terrain_generator;
pub mod tree;
