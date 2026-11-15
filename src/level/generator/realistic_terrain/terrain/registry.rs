//! 对应 `rtg/terrain/TerrainRegistry.php` 与 55 个 `TerrainVanilla*` 子类。
//!
//! PHP 里每个 biome 一个类，但绝大多数类的 `generateNoise()` 只有一行
//! “带不同常量参数调用 `TerrainBase::terrainXxx()`”。这里把这些常量直接内联成
//! 参数化结构体，`create()` 的 `match` 与 PHP 的 `switch` 一一对应，结果完全一致。
//!
//! PHP 里还有 4 个类没有被 registry 引用（`TerrainVanillaExtremeHills*`、
//! `TerrainRTGMesaBryce`），属于死代码，未移植。

use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::base;
use crate::level::generator::realistic_terrain::terrain::effects::{
    GroundEffect, HeightEffect, JitterEffect, RaiseEffect, SpikeEverywhereEffect,
    SummedHeightEffect, VoronoiBorderEffect, VoronoiPlateauEffect,
};
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;

pub trait Terrain: Send + Sync {
    /// `generateNoise($rtgWorld, $x, $y, $border, $river)`，`x/y` 是方块世界坐标。
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, border: f64, river: f64) -> f64;
}

/// `terrainBeach`：沙滩类（16/25/26）。
pub struct BeachTerrain;

impl Terrain for BeachTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_beach(x, y, world, river, 63.0))
    }
}

/// `riverized(base + GroundEffect(4))`：平原/森林/台地等一大票 biome。
pub struct FlatGroundTerrain {
    pub base: f64,
}

impl Terrain for FlatGroundTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        let ground = GroundEffect::new(4.0);
        f(base::riverized(
            f(self.base + ground.added(world, f(x), f(y))),
            river,
        ))
    }
}

pub struct FlatLakesTerrain {
    pub base: f64,
}

impl Terrain for FlatLakesTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_flat_lakes(x, y, world, river, self.base))
    }
}

pub struct HighlandTerrain {
    pub start: f64,
    pub width: f64,
    pub height: f64,
    pub base_adjust: f64,
}

impl Terrain for HighlandTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_highland(
            f(x),
            f(y),
            world,
            river,
            self.start,
            self.width,
            self.height,
            self.base_adjust,
        ))
    }
}

pub struct LonelyMountainTerrain {
    pub width: f64,
    pub strength: f64,
    pub terrain_height: f64,
}

impl Terrain for LonelyMountainTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_lonely_mountain(
            x,
            y,
            world,
            river,
            self.strength,
            self.width,
            self.terrain_height,
        ))
    }
}

pub struct PlainsTerrain {
    pub st_pitch: f64,
    pub st_factor: f64,
    pub h_pitch: f64,
    pub h_divisor: f64,
    pub base: f64,
}

impl Terrain for PlainsTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_plains(
            x,
            y,
            world,
            river,
            self.st_pitch,
            self.st_factor,
            self.h_pitch,
            self.h_divisor,
            self.base,
        ))
    }
}

pub struct GrasslandMountainsTerrain {
    pub h_factor: f64,
    pub m_factor: f64,
    pub base: f64,
}

impl Terrain for GrasslandMountainsTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_grassland_mountains(
            x,
            y,
            world,
            river,
            self.h_factor,
            self.m_factor,
            self.base,
        ))
    }
}

pub struct GrasslandFlatsTerrain {
    pub m_pitch: f64,
    pub base: f64,
}

impl Terrain for GrasslandFlatsTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_grassland_flats(
            x,
            y,
            world,
            river,
            self.m_pitch,
            self.base,
        ))
    }
}

pub struct GrasslandHillsTerrain {
    pub v_width: f64,
    pub v_height: f64,
    pub h_width: f64,
    pub h_height: f64,
    pub b_height: f64,
}

impl Terrain for GrasslandHillsTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_grassland_hills(
            x,
            y,
            world,
            river,
            self.v_width,
            self.v_height,
            self.h_width,
            self.h_height,
            self.b_height,
        ))
    }
}

pub struct MarshTerrain {
    pub base: f64,
}

impl Terrain for MarshTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_marsh(x, y, world, self.base, river))
    }
}

pub struct OceanTerrain {
    pub floor: f64,
}

impl Terrain for OceanTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        f(base::terrain_ocean(x, y, world, river, self.floor))
    }
}

pub struct PlateauTerrain {
    pub heights: &'static [f64],
    pub strength: f64,
    pub selector_wavelength: f64,
    pub is_m: bool,
}

impl Terrain for PlateauTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, border: f64, river: f64) -> f64 {
        f(base::terrain_plateau(
            f(x),
            f(y),
            world,
            river,
            self.heights,
            border,
            self.strength,
            self.heights.len(),
            self.selector_wavelength,
            self.is_m,
        ))
    }
}

/// 沙漠：沙丘高度由 `GeneratorSettings::sand_dune_height` 决定。
pub struct DesertTerrain {
    pub base: f64,
    pub min_dune_height: f64,
}

impl Terrain for DesertTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        let dune_height = f(self.min_dune_height + world.get_generator_settings().sand_dune_height);
        let dune_height =
            f(dune_height * f(f(1.0 + base::noise2f(world, 2, f(x / 330.0), f(y / 330.0))) / 2.0));
        f(base::terrain_polar(
            f(x),
            f(y),
            world,
            river,
            f(200.0),
            f(dune_height),
            f(70.0),
            f(40.0),
            self.base,
        ) + base::ground_noise(x, y, 1.0, world))
    }
}

/// 森林：地面噪声 + 丘陵，没有大起伏。
pub struct ForestTerrain {
    pub hill_strength: f64,
}

impl Terrain for ForestTerrain {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, river: f64) -> f64 {
        let ground_noise = f(base::ground_noise(x, y, 2.0, world));
        let m = f(base::hills(f(x), f(y), self.hill_strength, world));
        let flo_noise = f(f(65.0 + ground_noise) + m);
        f(base::riverized(flo_noise, river))
    }
}

/// 极端丘陵：脊状 Voronoi + 多层尖刺 + 抖动。
pub struct RidgedExtremeHills {
    pub base: f64,
    pub height_increase: Box<dyn HeightEffect>,
    pub multiplier: Box<dyn HeightEffect>,
    pub ground_effect: GroundEffect,
}

impl RidgedExtremeHills {
    pub fn new(land_height: f64, base_height: f64, hill_width: f64) -> Self {
        let height = f(land_height);
        let base_height = f(base_height);
        let width = f(hill_width);
        let ridge_width = f(300.0);

        let base_hills = SpikeEverywhereEffect {
            wavelength: f(width),
            minimum_simplex: f(-0.20000000298023224),
            octave: 3,
            power: f(1.899999976158142),
            spiked: Box::new(RaiseEffect::new(f(f(height * 2.0) / 3.0))),
        };

        let additional_wavelength = f(width / 3.0);
        let additional_spikes = SpikeEverywhereEffect {
            wavelength: additional_wavelength,
            minimum_simplex: f(-0.20000000298023224),
            octave: 4,
            power: f(1.899999976158142),
            spiked: Box::new(RaiseEffect::new(f(height / 3.0))),
        };
        let additional_height = JitterEffect::new(
            f(additional_wavelength / f(3.0)),
            f(additional_wavelength / f(2.0)),
            Box::new(additional_spikes),
        );

        let roughening_wavelength = f(width / 10.0);
        let roughening_spikes = SpikeEverywhereEffect {
            wavelength: roughening_wavelength,
            minimum_simplex: f(-0.20000000298023224),
            octave: 5,
            power: f(1.899999976158142),
            spiked: Box::new(RaiseEffect::new(f(height / 8.0))),
        };
        let roughening = JitterEffect::new(
            f(roughening_wavelength / f(3.0)),
            f(roughening_wavelength / f(2.0)),
            Box::new(roughening_spikes),
        );

        let stacked = SummedHeightEffect {
            one: Box::new(SummedHeightEffect {
                one: Box::new(base_hills),
                two: Box::new(additional_height),
            }),
            two: Box::new(roughening),
        };
        let height_increase = JitterEffect::new(15.0, 50.0, Box::new(stacked));

        let ridging = VoronoiBorderEffect {
            point_wavelength: f(ridge_width),
        };
        let ridge_jitter = JitterEffect::new(15.0, 50.0, Box::new(ridging));
        let multiplier = JitterEffect::new(5.0, 20.0, Box::new(ridge_jitter));

        Self {
            base: base_height,
            height_increase: Box::new(height_increase),
            multiplier: Box::new(multiplier),
            ground_effect: GroundEffect::new(6.0),
        }
    }
}

impl Terrain for RidgedExtremeHills {
    fn generate_noise(&self, world: &RtgWorld, x: f64, y: f64, _border: f64, _river: f64) -> f64 {
        let ground_level = f(self.ground_effect.added(world, f(x), f(y)));
        let mut ridging = f(self.multiplier.added(world, f(x), f(y)));
        ridging = f(base::bayesian_adjustment(ridging, f(2.0)));
        let result = f(f(self.base
            + f(ridging * f(ground_level + self.height_increase.added(world, f(x), f(y)))))
            + ground_level);
        f(base::mountain_cap(result))
    }
}

/// 台地（Mesa）：Voronoi 台阶 + 边界/河流收缩。
pub struct MesaPlateauTerrain {
    pub base: f64,
    pub ground_noise: f64,
    pub plateau: VoronoiPlateauEffect,
    pub jitter_wavelength: f64,
    pub jitter_amplitude: f64,
    pub bumpiness_multiplier: f64,
    pub bumpiness_wavelength: f64,
    pub variability_multiplier: f64,
    pub variability_wavelength: f64,
    pub variability_multiplier2: f64,
    pub variability_wavelength2: f64,
}

pub const MESA_STEP_START: f64 = 0.25;
pub const MESA_STEP_FINISH: f64 = 0.4000000059604645;
pub const MESA_STEP_HEIGHT: f64 = 32.0;

impl MesaPlateauTerrain {
    pub fn new(base: f64) -> Self {
        Self {
            base: f(base),
            ground_noise: 4.0,
            plateau: VoronoiPlateauEffect::new(200.0),
            jitter_wavelength: 30.0,
            jitter_amplitude: 10.0,
            bumpiness_multiplier: 0.10000000149011612,
            bumpiness_wavelength: 10.0,
            variability_multiplier: 0.15000000596046448,
            variability_wavelength: 30.0,
            variability_multiplier2: 0.07500000298023224,
            variability_wavelength2: 15.0,
        }
    }
}

/// `PlateauUtil::stepIncrease`
pub fn step_increase(value: f64, start: f64, finish: f64, height: f64) -> f64 {
    if value <= start {
        0.0
    } else if value >= finish {
        height
    } else {
        f(f(f(value - start) / f(finish - start)) * height)
    }
}

impl Terrain for MesaPlateauTerrain {
    fn generate_noise(
        &self,
        world: &RtgWorld,
        passed_x: f64,
        passed_y: f64,
        border: f64,
        river: f64,
    ) -> f64 {
        let delta = world.simplex_instance(1).multi_eval2d(
            f(f(passed_x) / self.jitter_wavelength),
            f(f(passed_y) / self.jitter_wavelength),
            false,
        );
        let x = f(passed_x + delta[0] * self.jitter_amplitude);
        let y = f(passed_y + delta[1] * self.jitter_amplitude);

        let bordercap = f(f(border * 3.5) - 2.5);
        let bordercap = f(if bordercap > f(1.0) { 1.0 } else { bordercap });
        let rivercap = f(3.0 * river);
        let rivercap = f(if rivercap > f(1.0) { 1.0 } else { rivercap });

        let bumpiness = f(base::noise2f(
            world,
            2,
            f(x / self.bumpiness_wavelength),
            f(y / self.bumpiness_wavelength),
        ) * self.bumpiness_multiplier);
        let variability = f(base::noise2f(
            world,
            3,
            f(x / self.variability_wavelength),
            f(y / self.variability_wavelength),
        ) * self.variability_multiplier);
        let variability2 = f(base::noise2f(
            world,
            3,
            f(x / self.variability_wavelength2),
            f(y / self.variability_wavelength2),
        ) * self.variability_multiplier2);
        let mut variability = f(variability + variability2);
        variability = f(variability.max(0.0));

        let mut base_simplex = f(self.plateau.added(world, x, y));
        base_simplex = f(base_simplex + variability);
        let simplex = f(f(f(base_simplex * bordercap) * rivercap) + bumpiness);

        let added = f(
            step_increase(simplex, MESA_STEP_START, MESA_STEP_FINISH, MESA_STEP_HEIGHT) / border,
        );

        f(base::riverized(
            f(self.base + base::ground_noise(x, y, f(self.ground_noise), world)),
            river,
        ) + added)
    }
}

/// 布莱斯台地：在 Mesa 的骨架上加石柱。
pub struct BrycePlateauTerrain {
    pub base: f64,
    pub ground_noise: f64,
    pub plateau: VoronoiPlateauEffect,
    pub jitter_wavelength: f64,
    pub jitter_amplitude: f64,
    pub bumpiness_multiplier: f64,
    pub bumpiness_wavelength: f64,
}

pub const BRYCE_STEP_START: f64 = 0.4000000059604645;
pub const BRYCE_STEP_FINISH: f64 = 0.699999988079071;
pub const BRYCE_HOODOO_STEP_START: f64 = 0.05000000074505806;
pub const BRYCE_HOODOO_STEP_FINISH: f64 = 0.44999998807907104;

impl BrycePlateauTerrain {
    pub fn new(base: f64) -> Self {
        Self {
            base: f(base),
            ground_noise: 4.0,
            plateau: VoronoiPlateauEffect::new(200.0),
            jitter_wavelength: 30.0,
            jitter_amplitude: 10.0,
            bumpiness_multiplier: 0.05000000074505806,
            bumpiness_wavelength: 10.0,
        }
    }

    fn hoodoo_height(world: &RtgWorld, x: f64, y: f64, height: f64) -> f64 {
        let mut sn = f(base::noise2f(world, 0, f(x / 2.0), f(y / 2.0)) * 0.5);
        sn = f(sn + base::noise2f(world, 0, x, y) * 0.2);
        sn = f(sn + f(base::noise2f(world, 0, f(x / 4.0), f(y / 4.0)) * 2.0));
        sn = f(sn + f(base::noise2f(world, 0, f(x / 8.0), f(y / 8.0)) * 2.0));
        sn = f(sn + f(base::noise2f(world, 0, f(x / 16.0), f(y / 16.0)) * 2.0));
        sn = f(f(sn * f(2.0)) / 6.699999809265137);
        sn = f(f(sn / f(2.0)) + 0.5);
        if sn < f(1.0) {
            sn = f(base::bayesian_adjustment(sn, 1.5));
        }
        sn = f(f(sn * 2.0) - 1.0);
        sn = f(sn - 0.4000000059604645);
        sn = f(sn / 0.6000000238418579);
        if sn < f(0.0) {
            sn = f(0.0);
        }
        f(height * sn)
    }
}

impl Terrain for BrycePlateauTerrain {
    fn generate_noise(
        &self,
        world: &RtgWorld,
        passed_x: f64,
        passed_y: f64,
        border: f64,
        river: f64,
    ) -> f64 {
        let delta = world.simplex_instance(1).multi_eval2d(
            f(f(passed_x) / self.jitter_wavelength),
            f(f(passed_y) / self.jitter_wavelength),
            false,
        );
        let x = f(passed_x + delta[0] * self.jitter_amplitude);
        let y = f(passed_y + delta[1] * self.jitter_amplitude);

        let bordercap = f(f(border * 3.5) - 2.5);
        let bordercap = f(if bordercap > f(1.0) { 1.0 } else { bordercap });
        let rivercap = f(3.0 * river);
        let rivercap = f(if rivercap > f(1.0) { 1.0 } else { rivercap });

        let bumpiness = f(base::noise2f(
            world,
            2,
            f(x / self.bumpiness_wavelength),
            f(y / self.bumpiness_wavelength),
        ) * self.bumpiness_multiplier);

        let simplex = f(f(f(self.plateau.added(world, x, y) * bordercap) * rivercap) + bumpiness);

        let mut added = f(step_increase(
            simplex,
            BRYCE_STEP_START,
            BRYCE_STEP_FINISH,
            MESA_STEP_HEIGHT,
        ) / border);
        let hoodoo_top = f(step_increase(
            simplex,
            BRYCE_HOODOO_STEP_START,
            BRYCE_HOODOO_STEP_FINISH,
            MESA_STEP_HEIGHT,
        ) / border);

        if hoodoo_top > added {
            added = f(added + Self::hoodoo_height(world, x, y, f(hoodoo_top - added)));
        }

        f(base::riverized(
            f(self.base + base::ground_noise(x, y, f(self.ground_noise), world)),
            river,
        ) + added)
    }
}

/// `TerrainRegistry::create`
pub fn create(id: i32) -> Option<Box<dyn Terrain>> {
    Some(match id {
        16 => Box::new(BeachTerrain),
        27 => Box::new(FlatGroundTerrain { base: 65.0 }),
        28 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 35.0,
            base_adjust: f(68.0 - 62.0),
        }),
        156 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 70.0,
            base_adjust: 10.0,
        }),
        155 => Box::new(PlainsTerrain {
            st_pitch: 160.0,
            st_factor: 10.0,
            h_pitch: 60.0,
            h_divisor: 80.0,
            base: 65.0,
        }),
        26 => Box::new(BeachTerrain),
        30 => Box::new(FlatLakesTerrain { base: 66.0 }),
        31 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 35.0,
            base_adjust: f(72.0 - 62.0),
        }),
        158 => Box::new(GrasslandMountainsTerrain {
            h_factor: 4.0,
            m_factor: 80.0,
            base: 68.0,
        }),
        24 => Box::new(OceanTerrain { floor: 40.0 }),
        2 => Box::new(DesertTerrain {
            base: 64.0,
            min_dune_height: base::MINIMUM_DUNE_HEIGHT,
        }),
        17 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 200.0,
            height: 80.0,
            base_adjust: f(68.0 - 62.0),
        }),
        130 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 200.0,
            height: 20.0,
            base_adjust: 10.0,
        }),
        3 | 34 => Box::new(RidgedExtremeHills::new(150.0, 67.0, 200.0)),
        20 => Box::new(RidgedExtremeHills::new(125.0, 67.0, 200.0)),
        131 | 162 => Box::new(RidgedExtremeHills::new(190.0, 67.0, 200.0)),
        132 => Box::new(PlainsTerrain {
            st_pitch: 160.0,
            st_factor: 10.0,
            h_pitch: 60.0,
            h_divisor: 80.0,
            base: 65.0,
        }),
        4 => Box::new(ForestTerrain {
            hill_strength: 10.0,
        }),
        18 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 30.0,
            base_adjust: f(72.0 - 62.0),
        }),
        10 | 0 => Box::new(OceanTerrain { floor: 50.0 }),
        11 | 7 => Box::new(FlatLakesTerrain { base: 60.0 }),
        13 => Box::new(LonelyMountainTerrain {
            width: 230.0,
            strength: 60.0,
            terrain_height: 68.0,
        }),
        12 | 140 => Box::new(PlainsTerrain {
            st_pitch: 160.0,
            st_factor: 10.0,
            h_pitch: 60.0,
            h_divisor: 200.0,
            base: 65.0,
        }),
        21 | 32 | 160 => Box::new(FlatLakesTerrain { base: 66.0 }),
        23 => Box::new(FlatGroundTerrain { base: 65.0 }),
        151 | 149 => Box::new(GrasslandMountainsTerrain {
            h_factor: 4.0,
            m_factor: 80.0,
            base: 68.0,
        }),
        22 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 40.0,
            base_adjust: f(72.0 - 62.0),
        }),
        33 | 161 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 40.0,
            base_adjust: f(72.0 - 62.0),
        }),
        37 => Box::new(FlatGroundTerrain { base: 68.0 }),
        165 => Box::new(BrycePlateauTerrain::new(67.0)),
        39 | 38 | 166 | 167 => Box::new(MesaPlateauTerrain::new(67.0)),
        14 => Box::new(GrasslandFlatsTerrain {
            m_pitch: 40.0,
            base: 68.0,
        }),
        15 | 6 => Box::new(MarshTerrain { base: 61.5 }),
        1 | 129 | 29 | 35 => Box::new(FlatGroundTerrain { base: 65.0 }),
        157 => Box::new(GrasslandMountainsTerrain {
            h_factor: 4.0,
            m_factor: 50.0,
            base: 68.0,
        }),
        163 => Box::new(PlateauTerrain {
            heights: &[
                18.0,
                0.4000000059604645,
                12.0,
                0.6000000238418579,
                8.0,
                0.800000011920929,
                4.0,
                0.800000011920929,
            ],
            strength: f(10.0),
            selector_wavelength: 50.0,
            is_m: true,
        }),
        36 => Box::new(PlateauTerrain {
            heights: &[12.0, 0.5, 6.0, 0.699999988079071],
            strength: f(10.0),
            selector_wavelength: 50.0,
            is_m: true,
        }),
        164 => Box::new(PlateauTerrain {
            heights: &[
                22.0,
                0.4000000059604645,
                16.0,
                0.6000000238418579,
                12.0,
                0.800000011920929,
            ],
            strength: f(10.0),
            selector_wavelength: 50.0,
            is_m: true,
        }),
        25 => Box::new(BeachTerrain),
        134 => Box::new(LonelyMountainTerrain {
            width: 50.0,
            strength: 25.0,
            terrain_height: 60.0,
        }),
        5 => Box::new(FlatLakesTerrain { base: 68.0 }),
        19 => Box::new(HighlandTerrain {
            start: 10.0,
            width: 68.0,
            height: 30.0,
            base_adjust: f(72.0 - 62.0),
        }),
        133 => Box::new(GrasslandHillsTerrain {
            v_width: 100.0,
            v_height: 7.0,
            h_width: 180.0,
            h_height: 70.0,
            b_height: 68.0,
        }),
        _ => return None,
    })
}
