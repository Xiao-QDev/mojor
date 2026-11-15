//! RTG 村庄结构：网格发现、不可变拓扑与逐区块放置。

pub mod map_generator;
pub mod placement;
pub mod placer;
pub mod topology;
pub mod village;

pub use map_generator::VillageMapGenerator;
pub use placer::{TerrainHeightProvider, generate};
pub use topology::{Direction, PieceKind, VillagePiece, VillageStructure};
