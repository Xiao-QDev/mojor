//! 20 个 `GenLayer*` 的具体实现，以及把它们串成链的
//! `initialize_all_biome_generators`（对应 PHP `GenLayer::initializeAllBiomeGenerators`）。
//!
//! 层的顺序是从 x86 版 `.so` 的 `Layer::getDefaultLayers` 反汇编里核对过的，
//! 和 JE 1.6.4 完全一致。

pub mod biomes;
pub mod climate;
pub mod land;
pub mod river;
pub mod zoom;

pub use biomes::{BiomeEdge, BiomeLayer, GenSettings, GeneratorType, Hills, RareBiome, Shore};
pub use climate::{AddSnow, Edge, EdgeMode};
pub use land::{AddIsland, AddMushroomIsland, DeepOcean, Island, RemoveTooMuchOcean};
pub use river::{River, RiverInit, RiverMix};
pub use zoom::{Smooth, VoronoiZoom, Zoom, magnify};

use super::gen_layer::Layer;

/// `GenLayer` 链的前半段：一直到 `DeepOcean(4)`。
///
/// PHP 里这棵树被三条支链共享（同一批对象）；Rust 侧改为每次重建一份。
fn build_base_tree() -> Box<dyn Layer> {
    let mut layer: Box<dyn Layer> = Box::new(Island::new(1));
    layer = Box::new(Zoom::fuzzy(2000, layer));
    layer = Box::new(AddIsland::new(1, layer));
    layer = Box::new(Zoom::new(2001, layer));
    layer = Box::new(AddIsland::new(2, layer));
    layer = Box::new(AddIsland::new(50, layer));
    layer = Box::new(AddIsland::new(70, layer));
    layer = Box::new(RemoveTooMuchOcean::new(2, layer));
    layer = Box::new(AddSnow::new(2, layer));
    layer = Box::new(AddIsland::new(3, layer));
    layer = Box::new(Edge::new(2, layer, EdgeMode::CoolWarm));
    layer = Box::new(Edge::new(2, layer, EdgeMode::HeatIce));
    layer = Box::new(Edge::new(3, layer, EdgeMode::Special));
    layer = Box::new(Zoom::new(2002, layer));
    layer = Box::new(Zoom::new(2003, layer));
    layer = Box::new(AddIsland::new(4, layer));
    layer = Box::new(AddMushroomIsland::new(5, layer));
    layer = Box::new(DeepOcean::new(4, layer));
    // PHP: GenLayerZoom::magnify(1000, $genlayerdeepocean, 0) —— 0 层，原样返回
    magnify(1000, layer, 0)
}

/// 构造一条完整的 `RiverMix` 链（biome 支路 + river 支路）。
fn build_mix_chain(
    seed: i64,
    kind: GeneratorType,
    settings: Option<GenSettings>,
    biome_size: i32,
    river_size: i32,
) -> Box<dyn Layer> {
    // 丘陵用的河流支路。PHP 里它的 `GenLayerRiverInit` 与主河流支链共享**同一个**
    // 对象，而主河流支链会被 `RiverMix::initWorldGenSeed` 播种 ——
    // 所以这个 `RiverInit`（以及它下面的基树）是播过种的；
    // 它上面那两层 `Zoom` 是丘陵支路自己的对象，没人给它播过种，保持原样。
    let mut river_init = Box::new(RiverInit::new(100, build_base_tree()));
    river_init.init_world_gen_seed(seed);
    let river_for_hills = magnify(1000, river_init, 2);

    let biome_layer = Box::new(BiomeLayer::new(200, build_base_tree(), kind, settings));
    let zoomed = magnify(1000, biome_layer, 2);
    let biome_edge = Box::new(BiomeEdge::new(1000, zoomed));

    let mut hills: Box<dyn Layer> = Box::new(Hills::new(1000, biome_edge, river_for_hills));
    hills = Box::new(RareBiome::new(1001, hills));

    for k in 0..biome_size {
        hills = Box::new(Zoom::new(1000 + k as i64, hills));
        if k == 0 {
            hills = Box::new(AddIsland::new(3, hills));
        }
        if k == 1 || biome_size == 1 {
            hills = Box::new(Shore::new(1000, hills));
        }
    }
    let smooth_with_biomes = Box::new(Smooth::new(1000, hills));

    // 主河流支链：整条都会被 `RiverMix::initWorldGenSeed` 播种，这里不用预处理
    let river_zoomed = magnify(1000, Box::new(RiverInit::new(100, build_base_tree())), 2);
    let river_zoomed = magnify(1000, river_zoomed, river_size);
    let river_layer = Box::new(River::new(1, river_zoomed));
    let smooth_river = Box::new(Smooth::new(1000, river_layer));

    Box::new(RiverMix::new(100, smooth_with_biomes, smooth_river))
}

/// `GenLayer::initializeAllBiomeGenerators`。
///
/// 返回 `(rivermix, voronoizoom)`：前者是 1/4 分辨率的生成层（用于找 biome 位置），
/// 后者是逐格 biome。
pub fn initialize_all_biome_generators(
    seed: i64,
    kind: GeneratorType,
    settings: Option<GenSettings>,
) -> (Box<dyn Layer>, Box<dyn Layer>) {
    let mut biome_size = 4;
    let mut river_size = 4;
    if let Some(s) = settings {
        biome_size = s.biome_size;
        river_size = s.river_size;
    }
    if kind == GeneratorType::LargeBiomes {
        biome_size = 6;
    }

    let mut mix = build_mix_chain(seed, kind, settings, biome_size, river_size);
    mix.init_world_gen_seed(seed);

    // PHP 里 voronoi 的 parent 就是同一个 rivermix 对象；
    // 两份副本在同一个 seed 下会算出完全相同的世界种子。
    let mut voronoi = Box::new(VoronoiZoom::new(
        10,
        build_mix_chain(seed, kind, settings, biome_size, river_size),
    ));
    voronoi.init_world_gen_seed(seed);

    (mix, voronoi)
}
