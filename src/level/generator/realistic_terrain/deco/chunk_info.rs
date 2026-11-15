//! 对应 `rtg/deco/ChunkInfo.php`。
//!
//! 每个区块一份，缓存"这个区块的树高噪声"。`DecoVariableTree` 靠它让同一片
//! 森林里的树高度接近——所以走进白桦林会觉得树是"一茬一茬"的。

use crate::level::generator::realistic_terrain::deco::offset_pos;
use crate::level::generator::realistic_terrain::terrain::world::RtgWorld;
use crate::level::generator::realistic_terrain::tree::f;

/// `ChunkInfo::$TREESIMPLEX`：树高用第 8 号 simplex。
const TREE_HEIGHT_SIMPLEX: usize = 8;

pub struct ChunkInfo {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub tree_height_noise_divisor: f64,
    stored_tree_height: Option<f64>,
}

impl ChunkInfo {
    pub fn new(chunk_x: i32, chunk_z: i32) -> Self {
        Self {
            chunk_x,
            chunk_z,
            tree_height_noise_divisor: 1237.0,
            stored_tree_height: None,
        }
    }

    /// 对应 `treeHeightNoiseValue`：第一次算完就缓存。
    pub fn tree_height_noise_value(&mut self, world: &RtgWorld) -> f64 {
        if self.stored_tree_height.is_none() {
            let offset = offset_pos(self.chunk_x, self.chunk_z);
            let noise = world.simplex_instance(TREE_HEIGHT_SIMPLEX);
            let nx = f(offset.x as f64) / self.tree_height_noise_divisor;
            let nz = f(offset.z as f64) / self.tree_height_noise_divisor;
            self.stored_tree_height = Some(noise.noise2f(nx as f32, nz as f32) as f64);
        }
        f(self.stored_tree_height.expect("刚写入"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_noise_is_cached_per_chunk() {
        let world = RtgWorld::new(4242);
        let mut info = ChunkInfo::new(3, -2);
        let first = info.tree_height_noise_value(&world);
        let second = info.tree_height_noise_value(&world);
        assert_eq!(first, second, "同一区块必须给出同一个树高噪声");

        let mut other = ChunkInfo::new(4, -2);
        assert_ne!(
            first,
            other.tree_height_noise_value(&world),
            "相邻区块应该有不同噪声"
        );
    }

    #[test]
    fn height_noise_stays_in_unit_range() {
        let world = RtgWorld::new(7);
        for chunk in -20..20 {
            let mut info = ChunkInfo::new(chunk, chunk * 3);
            let value = info.tree_height_noise_value(&world);
            assert!((-1.0..=1.0).contains(&value), "噪声 {value} 越界");
        }
    }
}
