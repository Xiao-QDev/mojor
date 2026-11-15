//! 对应 `rtg/terrain/`。
//!
//! | PHP | Rust |
//! |---|---|
//! | `RTGWorld.php` | [`world`] |
//! | `TerrainBase.php` | [`base`] |
//! | `*Effect.php`（13 个） | [`effects`] |
//! | `TerrainRegistry.php` + 55 个 `TerrainVanilla*` | [`registry`] |
//! | `WaterTerrain.php` | [`water`] |
//! | `BiomeProfiles.php` | [`profiles`] |
//! | `Point2.php` / `PlateauUtil.php` | 并入 [`effects`] / [`registry`] |
//!
//! 调用链：`LandscapeGenerator` 按 21×21 邻域加权，对每个参与混合的 biome
//! 调 `WaterTerrain::newr_noise()`，后者处理完湖泊/河流权重再进具体地形。

pub mod base;
pub mod effects;
pub mod profiles;
pub mod registry;
pub mod water;
pub mod world;

pub use registry::{Terrain, create};
pub use water::WaterTerrain;
pub use world::RtgWorld;

#[cfg(test)]
mod tests {
    #[test]
    fn profiles_and_registry_agree() {
        for id in 0..=255 {
            assert_eq!(
                super::profiles::get(id).is_some(),
                super::registry::create(id).is_some(),
                "biome {id} 在 profiles 与 registry 里的存在性不一致"
            );
        }
    }

    #[test]
    fn every_terrain_produces_finite_height() {
        let world = super::RtgWorld::new(1234);
        for id in 0..=255 {
            let Some(terrain) = super::registry::create(id) else {
                continue;
            };
            let profile = super::profiles::get(id).expect("与 registry 对齐");
            let water =
                super::WaterTerrain::new(terrain, profile.allow_rivers, profile.allow_lakes);

            for (x, z) in [(0.0, 0.0), (64.0, 32.0), (-500.0, 900.0)] {
                let river = super::base::get_river_strength(x as i32, z as i32, &world);
                let height = water.newr_noise(&world, x, z, 1.0, river + 1.0, None);
                assert!(
                    height.is_finite(),
                    "biome {id} 在 ({x},{z}) 产出了 {height}"
                );
                assert!(
                    (0.0..=255.0).contains(&height),
                    "biome {id} 在 ({x},{z}) 高度越界: {height}"
                );
            }
        }
    }
}
