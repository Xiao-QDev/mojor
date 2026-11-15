//! 对应 `rtg/terrain/WaterTerrain.php`。
//!
//! 在具体地形外面再包一层"河流 / 湖泊权重"处理：
//! 先按湖泊压力和河流强度合成一个压低系数，再交给地形本身，最后按湖底做侵蚀。

use crate::level::generator::realistic_terrain::math::f;
use crate::level::generator::realistic_terrain::terrain::base;
use crate::level::generator::realistic_terrain::terrain::registry::Terrain;
use crate::level::generator::realistic_terrain::terrain::world::{
    ACTUAL_RIVER_PROPORTION, LAKE_BOTTOM, RIVER_FLATTENING_ADDEND, RtgWorld,
};

pub struct WaterTerrain {
    pub terrain: Box<dyn Terrain>,
    pub allow_rivers: bool,
    pub allow_lakes: bool,
}

impl WaterTerrain {
    pub fn new(terrain: Box<dyn Terrain>, allow_rivers: bool, allow_lakes: bool) -> Self {
        Self {
            terrain,
            allow_rivers,
            allow_lakes,
        }
    }

    /// `newrNoise($rtgWorld, $x, $y, $border, $river, $columnLakePressure = null)`
    pub fn newr_noise(
        &self,
        world: &RtgWorld,
        x: f64,
        y: f64,
        border: f64,
        river: f64,
        column_lake_pressure: Option<f64>,
    ) -> f64 {
        if !self.allow_rivers {
            let mut border_for_river = f(border * f(2.0));
            if border_for_river > 1.0 {
                border_for_river = f(1.0);
            }
            let river = f(1.0 - f(f(1.0 - border_for_river) * f(1.0 - river)));
            return f(self.terrain.generate_noise(world, x, y, border, river));
        }

        let lake_strength = match column_lake_pressure {
            Some(pressure) => {
                if self.allow_lakes {
                    pressure
                } else {
                    1.0
                }
            }
            None => f(self.lake_pressure(
                world,
                x,
                y,
                world.get_lake_frequency(),
                world.get_lake_bend_size_large(),
                world.get_lake_bend_size_medium(),
                world.get_lake_bend_size_small(),
            )),
        };

        let mut adjusted_lake = f(self.lake_to_river_proportions(
            lake_strength,
            world.get_lake_shore_level(),
            world.get_lake_depression_level(),
        ));

        let mut river = river;
        if river < f(0.0) {
            river = f(0.0);
        }
        river = f(RtgWorld::river_adjusted_for_depth_difference(river));

        if adjusted_lake < ACTUAL_RIVER_PROPORTION {
            adjusted_lake = f(adjusted_lake - ACTUAL_RIVER_PROPORTION);
            adjusted_lake = f(adjusted_lake * 2.0);
            adjusted_lake = f(adjusted_lake + ACTUAL_RIVER_PROPORTION);
            if adjusted_lake < f(0.0) {
                adjusted_lake = f(0.0);
            }
        }

        if river < f(1.0) && adjusted_lake < f(1.0) {
            let least_lowering = f(adjusted_lake.min(river));
            river = f(f(f(1.0 - river) / river) + f(f(1.0 - adjusted_lake) / adjusted_lake));
            river = f(1.0 / f(river + 1.0));
            river = f(f(river + least_lowering) / 2.0);
        } else if adjusted_lake < river {
            river = f(adjusted_lake);
        }

        river = f(1.0 - river);
        river = f(river * f(f(river / f(river + 0.05000000074505806)) * 1.0499999523162842));
        river = f(1.0 - river);

        let mut river_flattening =
            f(f(river * f(1.0 + RIVER_FLATTENING_ADDEND)) - RIVER_FLATTENING_ADDEND);
        if river_flattening < f(0.0) {
            river_flattening = f(0.0);
        }

        let terrain_noise = f(self
            .terrain
            .generate_noise(world, x, y, border, river_flattening));

        f(self.eroded_noise(world, x, y, river, border, terrain_noise))
    }

    /// `erodedNoise`：湖泊低于河底时把地形压到湖底，并加一点不规则噪声。
    pub fn eroded_noise(
        &self,
        world: &RtgWorld,
        x: f64,
        y: f64,
        river: f64,
        _border: f64,
        biome_height: f64,
    ) -> f64 {
        let river_flattening = f(1.0 - river);
        let river_flattening = f(river_flattening - f(f(1.0) - ACTUAL_RIVER_PROPORTION));
        if river_flattening < f(0.0) {
            return f(biome_height);
        }
        let river_flattening = f(river_flattening / ACTUAL_RIVER_PROPORTION);
        let r = f(1.0 - river_flattening);

        if r < 1.0 && biome_height > LAKE_BOTTOM {
            let mut irregularity = f(f(base::noise2f(world, 0, f(x / 12.0), f(y / 12.0)) * 2.0)
                + base::noise2f(world, 0, f(x / 8.0), f(y / 8.0)));
            irregularity = f(irregularity * f(f(1.0) + r));
            return f(f(biome_height * r) + f(f(f(LAKE_BOTTOM + irregularity) * 1.0) * f(1.0 - r)));
        }
        f(biome_height)
    }

    /// 湖泊压力：三级 bend 抖动坐标后取 cellular 内部值。
    #[allow(clippy::too_many_arguments)]
    pub fn lake_pressure(
        &self,
        world: &RtgWorld,
        x: f64,
        y: f64,
        lake_interval: f64,
        large_bend_size: f64,
        medium_bend_size: f64,
        small_bend_size: f64,
    ) -> f64 {
        if !self.allow_lakes {
            return f(1.0);
        }

        let mut p_x = x;
        let mut p_y = y;

        // PHP 这里是 `F::div($x, 240.0)`，**没有**外层 `F::f`：
        // 抖动坐标参与的是 double 运算，只有后续的 cellular 取整才落地
        let delta = world
            .simplex_instance(1)
            .multi_eval2d(x / 240.0, y / 240.0, false);
        p_x += delta[0] * large_bend_size;
        p_y += delta[1] * large_bend_size;

        let delta = world
            .simplex_instance(0)
            .multi_eval2d(x / 80.0, y / 80.0, false);
        p_x += delta[0] * medium_bend_size;
        p_y += delta[1] * medium_bend_size;

        let delta = world
            .simplex_instance(4)
            .multi_eval2d(x / 30.0, y / 30.0, false);
        p_x += delta[0] * small_bend_size;
        p_y += delta[1] * small_bend_size;

        let lake = world.cellular_eval(0, p_x / lake_interval, p_y / lake_interval);
        f(1.0 - lake.interior_value())
    }

    /// 把湖泊压力映射成"压低比例"，中间段在河底与湖底之间线性过渡。
    pub fn lake_to_river_proportions(
        &self,
        pressure: f64,
        shore_level: f64,
        top_level: f64,
    ) -> f64 {
        if pressure > top_level {
            return f(1.0);
        }
        if pressure < shore_level {
            return f(f(pressure / shore_level) * ACTUAL_RIVER_PROPORTION);
        }
        let proportion = f(f(pressure - shore_level) / f(top_level - shore_level));
        f(ACTUAL_RIVER_PROPORTION + f(proportion * f(1.0 - ACTUAL_RIVER_PROPORTION)))
    }
}
