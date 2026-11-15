//! 对应 `rtg/biome/BiomeProvider.php`。
//!
//! 按 64×64 的方块区块取整张 biome 图并缓存，容量默认 128 块，
//! 超出后按插入顺序淘汰最老的一块（PHP 的 `unset($cache[key($cache)])`）。

use std::collections::{HashMap, VecDeque};

use crate::level::generator::realistic_terrain::math::JavaRandom;

use super::gen_layer::Layer;
use super::layers::{GenSettings, GeneratorType, initialize_all_biome_generators};

pub struct BiomeProvider {
    /// 逐格层（`VoronoiZoom`）
    layer: Box<dyn Layer>,
    /// 1/4 分辨率层（`RiverMix`），用于找 biome 位置
    generation_layer: Box<dyn Layer>,
    cache: HashMap<(i32, i32), Vec<u8>>,
    order: VecDeque<(i32, i32)>,
    capacity: usize,
}

impl BiomeProvider {
    pub fn new(seed: i64) -> Self {
        Self::with_capacity(seed, 128)
    }

    pub fn with_capacity(seed: i64, capacity: usize) -> Self {
        Self::with_settings(seed, capacity, GeneratorType::Default, None)
    }

    pub fn with_settings(
        seed: i64,
        capacity: usize,
        kind: GeneratorType,
        settings: Option<GenSettings>,
    ) -> Self {
        let (generation_layer, layer) = initialize_all_biome_generators(seed, kind, settings);
        Self {
            layer,
            generation_layer,
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn get_biome(&mut self, x: i32, z: i32) -> i32 {
        let tx = x >> 6;
        let tz = z >> 6;
        let key = (tx, tz);

        if !self.cache.contains_key(&key) {
            let data = self.layer.get_ints(tx << 6, tz << 6, 64, 64);
            let packed: Vec<u8> = data.iter().map(|&v| v as u8).collect();
            if self.cache.len() >= self.capacity {
                if let Some(oldest) = self.order.pop_front() {
                    self.cache.remove(&oldest);
                }
            }
            self.order.push_back(key);
            self.cache.insert(key, packed);
        }

        self.cache[&key][((z & 63) * 64 + (x & 63)) as usize] as i32
    }

    /// `BiomeProvider::areBiomesViable`：半径内的 1/4 分辨率格是否全在允许集合里。
    pub fn are_biomes_viable(&mut self, x: i32, z: i32, radius: i32, allowed: &[i32]) -> bool {
        let min_x = (x - radius) >> 2;
        let min_z = (z - radius) >> 2;
        let max_x = (x + radius) >> 2;
        let max_z = (z + radius) >> 2;
        let data =
            self.generation_layer
                .get_ints(min_x, min_z, max_x - min_x + 1, max_z - min_z + 1);
        data.iter().all(|id| allowed.contains(id))
    }

    /// `BiomeProvider::findBiomePosition`：在范围内随机挑一个允许的 biome 位置。
    pub fn find_biome_position(
        &mut self,
        x: i32,
        z: i32,
        range: i32,
        allowed: &[i32],
        random: &mut JavaRandom,
    ) -> Option<[i32; 2]> {
        let min_x = (x - range) >> 2;
        let min_z = (z - range) >> 2;
        let max_x = (x + range) >> 2;
        let max_z = (z + range) >> 2;
        let width = max_x - min_x + 1;
        let data = self
            .generation_layer
            .get_ints(min_x, min_z, width, max_z - min_z + 1);

        let mut result = None;
        let mut matches = 0;
        for (index, id) in data.iter().enumerate() {
            if allowed.contains(id) && (result.is_none() || random.next_int_bound(matches + 1) == 0)
            {
                let index = index as i32;
                result = Some([(min_x + index % width) << 2, (min_z + index / width) << 2]);
                matches += 1;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_varied_and_reproducible() {
        let mut a = BiomeProvider::new(1234);
        let mut b = BiomeProvider::new(1234);
        let mut seen = std::collections::HashSet::new();

        for x in (0..256).step_by(4) {
            for z in (0..256).step_by(4) {
                let id = a.get_biome(x, z);
                assert_eq!(id, b.get_biome(x, z), "同种子必须可复现");
                seen.insert(id);
            }
        }

        // 全 0 或全 -1 说明链接没接上
        assert!(seen.len() > 3, "biome 种类过少: {:?}", seen);
    }
}
