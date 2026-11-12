// 世界层 目录结构对齐 Nukkit 的 level 包
// level 是入口 ChunkManager 管区块 ChunkLoader 管存档 format 管区块格式 generator 管地形
pub mod actor;
pub mod chunk_loader;
pub mod chunk_io;
pub mod chunk_manager;
pub mod distance;
pub mod format;
pub mod generator;
pub mod light;
pub mod region;
pub mod region_handler;
// 与 Nukkit 一致 Level 放在 level 包下 模块名与父模块同名
#[allow(clippy::module_inception)]
pub mod level;

pub use chunk_loader::ChunkLoader;
pub use level::Level;
