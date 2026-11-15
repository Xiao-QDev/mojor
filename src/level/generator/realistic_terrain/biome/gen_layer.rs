//! 对应 `rtg/biome/grid/GenLayer.php`。
//!
//! 这是 JE 那套「分层缩放」biome 生成器的基类：
//!
//! ```text
//! baseSeed     = mix(mix(mix(seed, seed), seed), seed)
//! worldGenSeed = initWorldGenSeed 时沿父链下发，再用 baseSeed 混三次
//! chunkSeed    = mix(mix(mix(mix(worldGenSeed, x), z), x), z)
//! nextInt(n)   = ((chunkSeed >> 24) % n + n) % n，然后把 chunkSeed 与 worldGenSeed 混合
//! ```
//!
//! PHP 用 `IntCache` 复用整型数组；Rust 侧每次 `vec![0; w*h]` 即可，
//! 因为每格都会被完整写入，复用池不影响结果。

use crate::level::generator::realistic_terrain::math::mix;

/// 所有层共有的状态。
pub struct LayerBase {
    /// 上游层，`IslandLayer` 为 `None`
    pub parent: Option<Box<dyn Layer>>,
    base_seed: i64,
    world_gen_seed: i64,
    chunk_seed: i64,
}

impl LayerBase {
    pub fn new(seed: i64) -> Self {
        let mut base_seed = seed;
        base_seed = mix(base_seed, seed);
        base_seed = mix(base_seed, seed);
        base_seed = mix(base_seed, seed);
        Self {
            parent: None,
            base_seed,
            world_gen_seed: 0,
            chunk_seed: 0,
        }
    }

    pub fn with_parent(seed: i64, parent: Box<dyn Layer>) -> Self {
        let mut base = Self::new(seed);
        base.parent = Some(parent);
        base
    }

    /// `initWorldGenSeed` 的尾部：世界种子与本层 `baseSeed` 混合三次。
    pub fn apply_seed(&mut self, seed: i64) {
        let mut seed = seed;
        seed = mix(seed, self.base_seed);
        seed = mix(seed, self.base_seed);
        seed = mix(seed, self.base_seed);
        self.world_gen_seed = seed;
    }
}

pub trait Layer: Send + Sync {
    fn base(&self) -> &LayerBase;
    fn base_mut(&mut self) -> &mut LayerBase;

    /// 子类实现：按 `(areaX, areaY, width, height)` 输出 biome id，行优先。
    fn generate_ints(&mut self, area_x: i32, area_y: i32, width: i32, height: i32) -> Vec<i32>;

    fn get_ints(&mut self, x: i32, z: i32, width: i32, height: i32) -> Vec<i32> {
        self.generate_ints(x, z, width, height)
    }

    /// 沿父链下发世界种子，再与本层 `baseSeed` 混合三次。
    fn init_world_gen_seed(&mut self, seed: i64) {
        if let Some(parent) = self.base_mut().parent.as_mut() {
            parent.init_world_gen_seed(seed);
        }
        self.base_mut().apply_seed(seed);
    }

    fn init_chunk_seed(&mut self, x: i32, z: i32) {
        let base = self.base_mut();
        let x = x as i64;
        let z = z as i64;
        base.chunk_seed = mix(mix(mix(mix(base.world_gen_seed, x), z), x), z);
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        let base = self.base_mut();
        let mut i = ((base.chunk_seed >> 24) % bound as i64) as i32;
        if i < 0 {
            i += bound;
        }
        base.chunk_seed = mix(base.chunk_seed, base.world_gen_seed);
        i
    }

    /// 从若干候选里按 `nextInt(len)` 取一个。
    fn select_random(&mut self, options: &[i32]) -> i32 {
        let index = self.next_int(options.len() as i32) as usize;
        options[index]
    }

    /// JE 的 `selectModeOrRandom`：众数优先，平票才随机。
    fn select_mode_or_random(&mut self, a: i32, b: i32, c: i32, d: i32) -> i32 {
        if b == c && c == d {
            b
        } else if a == b && a == c {
            a
        } else if a == b && a == d {
            a
        } else if a == c && a == d {
            a
        } else if a == b && c != d {
            a
        } else if a == c && b != d {
            a
        } else if a == d && b != c {
            a
        } else if b == c && a != d {
            b
        } else if b == d && a != c {
            b
        } else if c == d && a != b {
            c
        } else {
            self.select_random(&[a, b, c, d])
        }
    }
}

/// 取父层结果，调用方保证 `parent` 存在。
pub fn parent_ints(layer: &mut dyn Layer, x: i32, z: i32, width: i32, height: i32) -> Vec<i32> {
    layer
        .base_mut()
        .parent
        .as_mut()
        .expect("该层必须有父层")
        .get_ints(x, z, width, height)
}

/// 海洋判定：0 海洋、24 深海、10 冻洋。
#[inline]
pub fn is_biome_oceanic(id: i32) -> bool {
    id == 0 || id == 24 || id == 10
}
