//! 村庄起点发现、拓扑缓存和 shifted-population 检索。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::level::generator::realistic_terrain::biome::BiomeProvider;
use crate::level::generator::realistic_terrain::math::JavaRandom;

use super::placement::{StructurePlacement, VILLAGE_DISTANCE, population_clip};
use super::topology::VillageStructure;

pub struct VillageMapGenerator {
    seed: i64,
    provider: BiomeProvider,
    placement: StructurePlacement,
    seed_x: i64,
    seed_z: i64,
    cache: HashMap<(i32, i32), Option<Arc<VillageStructure>>>,
    order: VecDeque<(i32, i32)>,
    capacity: usize,
}

impl VillageMapGenerator {
    pub fn new(seed: i64) -> Self {
        let mut random = JavaRandom::new(seed);
        let seed_x = random.next_long();
        let seed_z = random.next_long();
        Self {
            seed,
            provider: BiomeProvider::new(seed),
            placement: StructurePlacement::new(seed, VILLAGE_DISTANCE),
            seed_x,
            seed_z,
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity: 128,
        }
    }

    pub fn start_at(&mut self, chunk_x: i32, chunk_z: i32) -> Option<Arc<VillageStructure>> {
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        let source = (chunk_x as i64).wrapping_mul(self.seed_x)
            ^ (chunk_z as i64).wrapping_mul(self.seed_z)
            ^ self.seed;
        let mut random = JavaRandom::new(source);
        // MapGenVillage::getStructureStart 在 biome 检查前丢弃一次 nextInt。
        random.next_int();
        let start = if self
            .placement
            .can_village_spawn(&mut self.provider, chunk_x, chunk_z)
        {
            let candidate =
                VillageStructure::create(&mut random, &mut self.provider, chunk_x, chunk_z, 0);
            candidate.valid.then(|| Arc::new(candidate))
        } else {
            None
        };
        self.cache.insert(key, start.clone());
        self.order.push_back(key);
        if self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.cache.remove(&oldest);
            }
        }
        start
    }

    pub fn starts_for_population_chunk(&mut self, cx: i32, cz: i32) -> Vec<Arc<VillageStructure>> {
        let clip = population_clip(cx, cz);
        let candidates = self.placement.candidates_in_window(
            cx.wrapping_sub(8),
            cz.wrapping_sub(8),
            cx.wrapping_add(8),
            cz.wrapping_add(8),
        );
        let mut result = Vec::new();
        for (x, z) in candidates {
            if let Some(start) = self.start_at(x, z) {
                if start.bounding_box.intersects(&clip) {
                    result.push(start);
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_seed_derivation_is_repeatable() {
        let a = VillageMapGenerator::new(-9_223_372_036_854_700_000);
        let b = VillageMapGenerator::new(-9_223_372_036_854_700_000);
        assert_eq!((a.seed_x, a.seed_z), (b.seed_x, b.seed_z));
        assert_ne!(a.seed_x, a.seed_z);
    }
}
