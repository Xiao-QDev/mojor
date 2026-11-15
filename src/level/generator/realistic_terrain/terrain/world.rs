//! 对应 `rtg/RTGWorld.php`。
//!
//! PHP 里 `simplexInstance()` / `cellularInstance()` 是**惰性创建**的，
//! 这里改为构造时就建好 10 个 simplex 和 5 个 cellular。
//! 每个实例的种子都是 `worldSeed + i`，是纯函数，所以两者结果完全一致。
//!
//! `SpacedCellularNoise::eval2d` 需要 `&mut self`（内部有 256 项的点集缓存），
//! 因此用 `RefCell` 包一层。RTG 的世界实例本来就是单线程独占的。

use std::cell::RefCell;

use crate::level::generator::realistic_terrain::math::{JavaRandom, f};
use crate::level::generator::realistic_terrain::noise::{
    OpenSimplexNoise, SpacedCellularNoise, VoronoiResult,
};

pub const ACTUAL_RIVER_PROPORTION: f64 = 0.09375;
pub const RIVER_FLATTENING_ADDEND: f64 = 0.1034482792019844;
pub const RIVER_BOTTOM: f64 = 57.0;
pub const LAKE_BOTTOM: f64 = 53.0;

const SIMPLEX_COUNT: usize = 10;
const CELLULAR_COUNT: usize = 5;

/// PHP 里是个匿名对象字面量，只有 `sandDuneHeight` 真的被地形用到。
#[derive(Clone, Copy, Debug)]
pub struct GeneratorSettings {
    pub sand_dune_height: f64,
    pub river_cut_off_scale: f64,
    pub river_cut_off_ampl: f64,
    pub use_boulders: bool,
}

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self {
            sand_dune_height: 4.0,
            river_cut_off_scale: 350.0,
            river_cut_off_ampl: 0.5,
            use_boulders: true,
        }
    }
}

pub struct RtgWorld {
    seed: i64,
    simplex: Vec<OpenSimplexNoise>,
    cellular: Vec<RefCell<SpacedCellularNoise>>,
    random: std::cell::RefCell<JavaRandom>,
    settings: GeneratorSettings,
    chunk_seed: Option<(i64, i64)>,
}

impl RtgWorld {
    pub fn new(seed: i64) -> Self {
        Self {
            seed,
            simplex: (0..SIMPLEX_COUNT)
                .map(|i| OpenSimplexNoise::new(seed.wrapping_add(i as i64)))
                .collect(),
            cellular: (0..CELLULAR_COUNT)
                .map(|i| RefCell::new(SpacedCellularNoise::new(seed.wrapping_add(i as i64))))
                .collect(),
            random: std::cell::RefCell::new(JavaRandom::new(seed)),
            settings: GeneratorSettings::default(),
            chunk_seed: None,
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    /// `simplexInstance($i)`：`$i >= 10` 时回落到 0。
    pub fn simplex_instance(&self, i: usize) -> &OpenSimplexNoise {
        &self.simplex[if i >= SIMPLEX_COUNT { 0 } else { i }]
    }

    /// `cellularInstance($i)->eval2D(x, y)`：`$i >= 5` 时回落到 0。
    pub fn cellular_eval(&self, i: usize, x: f64, y: f64) -> VoronoiResult {
        let index = if i >= CELLULAR_COUNT { 0 } else { i };
        self.cellular[index].borrow_mut().eval2d(x, y)
    }

    /// 共享的 RTG 随机源。PHP 里地表层、洞穴层、种群层都拿同一个 `$rtgWorld->rand()`，
    /// 所以这里用 `RefCell`，让 `&self` 也能取到它（和 `simplex_instance` 同时借用不冲突）。
    pub fn rand(&self) -> std::cell::RefMut<'_, JavaRandom> {
        self.random.borrow_mut()
    }

    pub fn get_generator_settings(&self) -> &GeneratorSettings {
        &self.settings
    }

    pub fn get_chunk_seed(&mut self, chunk_x: i64, chunk_z: i64) -> i64 {
        if self.chunk_seed.is_none() {
            let mut random = JavaRandom::new(self.seed);
            let x = (random.next_long() / 2) * 2 + 1;
            let z = (random.next_long() / 2) * 2 + 1;
            self.chunk_seed = Some((x, z));
        }
        let (seed_x, seed_z) = self.chunk_seed.expect("seeded above");
        chunk_x
            .wrapping_mul(seed_x)
            .wrapping_add(chunk_z.wrapping_mul(seed_z))
            ^ self.seed
    }

    pub fn get_river_large_bend_size(&self) -> f64 {
        140.0
    }

    pub fn get_river_small_bend_size(&self) -> f64 {
        30.0
    }

    pub fn get_river_separation(&self) -> f64 {
        975.0
    }

    pub fn get_river_valley_level(&self) -> f64 {
        140.0 / 450.0
    }

    pub fn get_lake_frequency(&self) -> f64 {
        649.0
    }

    pub fn get_lake_shore_level(&self) -> f64 {
        f(0.035)
    }

    pub fn get_lake_depression_level(&self) -> f64 {
        f(0.15)
    }

    pub fn get_lake_bend_size_large(&self) -> f64 {
        80.0
    }

    pub fn get_lake_bend_size_medium(&self) -> f64 {
        30.0
    }

    pub fn get_lake_bend_size_small(&self) -> f64 {
        12.0
    }

    /// 河道深度差补偿：湖泊比河底浅，所以比例要按深度重新映射。
    pub fn river_adjusted_for_depth_difference(river: f64) -> f64 {
        if river > ACTUAL_RIVER_PROPORTION {
            return river;
        }
        let river = f(river - ACTUAL_RIVER_PROPORTION);
        let river = f(f(river * (63.0 - RIVER_BOTTOM)) / (63.0 - LAKE_BOTTOM));
        f(river + ACTUAL_RIVER_PROPORTION)
    }
}
