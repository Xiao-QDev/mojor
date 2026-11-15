//! 村庄结构包围盒与带盐网格候选点。

use crate::level::generator::realistic_terrain::biome::BiomeProvider;
use crate::level::generator::realistic_terrain::math::JavaRandom;

pub const VILLAGE_BIOMES: [i32; 4] = [1, 2, 35, 5];
pub const VILLAGE_DISTANCE: i32 = 32;
pub const VILLAGE_SEPARATION: i32 = 8;
pub const VILLAGE_SALT: i64 = 10_387_312;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoundingBox {
    pub min_x: i32,
    pub min_y: i32,
    pub min_z: i32,
    pub max_x: i32,
    pub max_y: i32,
    pub max_z: i32,
}

impl BoundingBox {
    pub const fn new(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> Self {
        Self {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.max_x >= other.min_x
            && self.min_x <= other.max_x
            && self.max_z >= other.min_z
            && self.min_z <= other.max_z
            && self.max_y >= other.min_y
            && self.min_y <= other.max_y
    }

    pub fn expand_to(&mut self, other: &Self) {
        self.min_x = self.min_x.min(other.min_x);
        self.min_y = self.min_y.min(other.min_y);
        self.min_z = self.min_z.min(other.min_z);
        self.max_x = self.max_x.max(other.max_x);
        self.max_y = self.max_y.max(other.max_y);
        self.max_z = self.max_z.max(other.max_z);
    }

    pub fn offset(&mut self, x: i32, y: i32, z: i32) {
        self.min_x += x;
        self.min_y += y;
        self.min_z += z;
        self.max_x += x;
        self.max_y += y;
        self.max_z += z;
    }

    pub fn x_size(&self) -> i32 {
        self.max_x - self.min_x + 1
    }
    pub fn y_size(&self) -> i32 {
        self.max_y - self.min_y + 1
    }
    pub fn z_size(&self) -> i32 {
        self.max_z - self.min_z + 1
    }
    pub fn values(&self) -> [i32; 6] {
        [
            self.min_x, self.min_y, self.min_z, self.max_x, self.max_y, self.max_z,
        ]
    }
}

#[derive(Clone, Debug)]
pub struct SpacedStructureGrid {
    seed: i64,
    spacing: i32,
    separation: i32,
    salt: i64,
    triangular: bool,
}

impl SpacedStructureGrid {
    pub fn new(seed: i64, spacing: i32, separation: i32, salt: i64, triangular: bool) -> Self {
        assert!(spacing >= 1 && separation >= 0 && separation < spacing);
        Self {
            seed,
            spacing,
            separation,
            salt,
            triangular,
        }
    }

    pub fn village(seed: i64, distance: i32) -> Self {
        Self::new(
            seed,
            distance.max(9),
            VILLAGE_SEPARATION,
            VILLAGE_SALT,
            false,
        )
    }

    pub fn candidate(&self, chunk_x: i32, chunk_z: i32) -> (i32, i32) {
        // Java/PHP 整数除法向零取整，负区块先补一个 region。
        let source_x = if chunk_x < 0 {
            chunk_x as i64 - (self.spacing - 1) as i64
        } else {
            chunk_x as i64
        };
        let source_z = if chunk_z < 0 {
            chunk_z as i64 - (self.spacing - 1) as i64
        } else {
            chunk_z as i64
        };
        self.candidate_in_region(
            (source_x / self.spacing as i64) as i32,
            (source_z / self.spacing as i64) as i32,
        )
    }

    pub fn candidate_in_region(&self, region_x: i32, region_z: i32) -> (i32, i32) {
        let seed = (region_x as i64)
            .wrapping_mul(341_873_128_712)
            .wrapping_add((region_z as i64).wrapping_mul(132_897_987_541))
            .wrapping_add(self.seed)
            .wrapping_add(self.salt);
        let mut random = JavaRandom::new(seed);
        let bound = self.spacing - self.separation;
        let x = (region_x as i64 * self.spacing as i64) as i32;
        let z = (region_z as i64 * self.spacing as i64) as i32;
        if self.triangular {
            let dx = (random.next_int_bound(bound) + random.next_int_bound(bound)) / 2;
            let dz = (random.next_int_bound(bound) + random.next_int_bound(bound)) / 2;
            (x.wrapping_add(dx), z.wrapping_add(dz))
        } else {
            (
                x.wrapping_add(random.next_int_bound(bound)),
                z.wrapping_add(random.next_int_bound(bound)),
            )
        }
    }

    pub fn candidates_in_window(
        &self,
        min_x: i32,
        min_z: i32,
        max_x: i32,
        max_z: i32,
    ) -> Vec<(i32, i32)> {
        let mut result = Vec::new();
        let spacing = self.spacing as i64;
        for region_x in (min_x as i64).div_euclid(spacing)..=(max_x as i64).div_euclid(spacing) {
            for region_z in (min_z as i64).div_euclid(spacing)..=(max_z as i64).div_euclid(spacing)
            {
                let candidate = self.candidate_in_region(region_x as i32, region_z as i32);
                if candidate.0 >= min_x
                    && candidate.0 <= max_x
                    && candidate.1 >= min_z
                    && candidate.1 <= max_z
                {
                    result.push(candidate);
                }
            }
        }
        result.sort_unstable_by_key(|&(x, z)| (x, z));
        result
    }
}

#[derive(Clone, Debug)]
pub struct StructurePlacement {
    village_grid: SpacedStructureGrid,
}

impl StructurePlacement {
    pub fn new(seed: i64, distance: i32) -> Self {
        Self {
            village_grid: SpacedStructureGrid::village(seed, distance),
        }
    }

    pub fn village_candidate(&self, chunk_x: i32, chunk_z: i32) -> (i32, i32) {
        self.village_grid.candidate(chunk_x, chunk_z)
    }

    pub fn candidates_in_window(
        &self,
        min_x: i32,
        min_z: i32,
        max_x: i32,
        max_z: i32,
    ) -> Vec<(i32, i32)> {
        self.village_grid
            .candidates_in_window(min_x, min_z, max_x, max_z)
    }

    pub fn can_village_spawn(
        &self,
        provider: &mut BiomeProvider,
        chunk_x: i32,
        chunk_z: i32,
    ) -> bool {
        self.village_candidate(chunk_x, chunk_z) == (chunk_x, chunk_z)
            && provider.are_biomes_viable(
                chunk_x.wrapping_mul(16).wrapping_add(8),
                chunk_z.wrapping_mul(16).wrapping_add(8),
                0,
                &VILLAGE_BIOMES,
            )
    }
}

pub fn population_clip(chunk_x: i32, chunk_z: i32) -> BoundingBox {
    BoundingBox::new(
        chunk_x.wrapping_mul(16).wrapping_add(8),
        1,
        chunk_z.wrapping_mul(16).wrapping_add(8),
        chunk_x.wrapping_mul(16).wrapping_add(23),
        512,
        chunk_z.wrapping_mul(16).wrapping_add(23),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_edges_expand_offset_and_sizes() {
        let mut a = BoundingBox::new(-3, 2, 4, 2, 8, 9);
        assert!(a.intersects(&BoundingBox::new(2, 8, 9, 4, 10, 12)));
        assert!(!a.intersects(&BoundingBox::new(3, 2, 4, 4, 8, 9)));
        assert_eq!([a.x_size(), a.y_size(), a.z_size()], [6, 7, 6]);
        a.expand_to(&BoundingBox::new(-8, 1, 7, 0, 12, 20));
        assert_eq!(a.values(), [-8, 1, 4, 2, 12, 20]);
        a.offset(-2, 3, -4);
        assert_eq!(a.values(), [-10, 4, 0, 0, 15, 16]);
    }

    #[test]
    fn negative_chunks_use_floor_regions() {
        let grid = SpacedStructureGrid::village(7, 32);
        assert_eq!(grid.candidate(-1, -1), grid.candidate_in_region(-1, -1));
        assert_eq!(grid.candidate(-32, -32), grid.candidate_in_region(-1, -1));
        assert_eq!(grid.candidate(-33, -33), grid.candidate_in_region(-2, -2));
        assert_eq!(grid.candidate(0, 0), grid.candidate_in_region(0, 0));
        let window = grid.candidates_in_window(-40, -40, 40, 40);
        assert!(window.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(
            window
                .iter()
                .all(|&(x, z)| (-40..=40).contains(&x) && (-40..=40).contains(&z))
        );
    }

    #[test]
    fn village_constants_and_clip_match_population_shift() {
        let grid = SpacedStructureGrid::village(1, 2);
        assert_eq!(grid.spacing, 9);
        assert_eq!(grid.separation, 8);
        assert_eq!(grid.salt, 10_387_312);
        assert_eq!(VILLAGE_DISTANCE, 32);
        assert_eq!(population_clip(-1, -2).values(), [-8, 1, -24, 7, 512, -9]);
    }
}
