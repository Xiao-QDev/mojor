//! 对应 `rtg/carver/`：洞穴与峡谷。
//!
//! PHP 侧是三层包装：`MapGenCaves`（Mojang 原算法）→ `MapGenCavesRTG`
//! （把 `nextInt(15)/nextInt(7)` 换成可配的 density/chance）→
//! `MemoizedCaves`（按隧道缓存每步几何）→ `CachedCaves`（按 origin 缓存隧道计划）。
//! 后两层只是性能缓存、不影响数值，这里直接实现最内层，参数取 RTG 的默认值
//! （vanilla 模式下 `caveChance=7`、`caveDensity=15` 与原算法等值）。
//!
//! 每个区块要把周围 `range=8` 的 17×17 个 origin 各跑一遍 —— origin 之间
//! 靠 `setSeed` 隔离，所以可以任意顺序、任意重复执行。

pub mod caves;
pub mod math_helper;
pub mod ravines;

use std::cell::RefCell;
use std::collections::HashMap;

use crate::level::generator::realistic_terrain::biome::BiomeProvider;

/// 对应 `carver/CarverWorld.php`：carver 只需要世界种子和「某列 biome 的 topBlock」。
pub struct CarverWorld {
    seed: i64,
    biomes: RefCell<BiomeProvider>,
    tops: RefCell<HashMap<i32, i32>>,
}

impl CarverWorld {
    pub fn new(seed: i64) -> Self {
        Self {
            seed,
            biomes: RefCell::new(BiomeProvider::new(seed)),
            tops: RefCell::new(HashMap::new()),
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    /// PHP 里 `$world->getBiome($pos)->topBlock`，按 biome id 缓存。
    pub fn top_block(&self, x: i32, z: i32) -> i32 {
        let biome = self.biomes.borrow_mut().get_biome(x, z);
        let mut tops = self.tops.borrow_mut();
        *tops.entry(biome).or_insert_with(|| biome_top_block(biome))
    }
}

/// `CarverWorld::getBiome` 里那张表。注意它与 `SurfaceRegistry` 的
/// `$topBlock` 特例是两份独立代码，别合并。
fn biome_top_block(id: i32) -> i32 {
    if [2, 17, 130, 16, 26].contains(&id) {
        192
    } else if id == 25 {
        16
    } else if id == 140 {
        1280
    } else if id == 14 || id == 15 {
        1760
    } else if [37, 38, 39, 165, 166, 167].contains(&id) {
        193
    } else {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_block_table() {
        assert_eq!(biome_top_block(1), 32);
        assert_eq!(biome_top_block(16), 192);
        assert_eq!(biome_top_block(25), 16);
        assert_eq!(biome_top_block(140), 1280);
        assert_eq!(biome_top_block(14), 1760);
        assert_eq!(biome_top_block(167), 193);
    }
}
