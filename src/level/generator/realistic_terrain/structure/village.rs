//! 村庄模块的稳定集成入口。

pub use super::map_generator::VillageMapGenerator;
pub use super::placer::TerrainHeightProvider;
pub use super::topology::{Direction, PieceKind, VillagePiece, VillageStructure};

/// 与集成侧 `structure::village::placer::generate` 路径保持一致。
pub mod placer {
    pub use super::super::placer::*;
}
