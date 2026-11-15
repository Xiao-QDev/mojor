//! `GenLayerRiverInit` / `GenLayerRiver` / `GenLayerRiverMix`。

use crate::level::generator::realistic_terrain::biome::gen_layer::{Layer, LayerBase, parent_ints};

/// 给陆地格打上河源强度（2..300000），海洋为 0。
pub struct RiverInit {
    base: LayerBase,
}

impl RiverInit {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for RiverInit {
    fn base(&self) -> &LayerBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LayerBase {
        &mut self.base
    }

    fn generate_ints(
        &mut self,
        area_x: i32,
        area_y: i32,
        area_width: i32,
        area_height: i32,
    ) -> Vec<i32> {
        let aint = parent_ints(self, area_x, area_y, area_width, area_height);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                aint1[(j + i * area_width) as usize] = if aint[(j + i * area_width) as usize] > 0 {
                    self.next_int(299999) + 2
                } else {
                    0
                };
            }
        }
        aint1
    }
}

/// 河源值收敛成河道：四邻一致则为 -1，否则是河流 biome（7）。
pub struct River {
    base: LayerBase,
}

impl River {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }

    fn filter(value: i32) -> i32 {
        if value >= 2 { 2 + (value & 1) } else { value }
    }
}

impl Layer for River {
    fn base(&self) -> &LayerBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LayerBase {
        &mut self.base
    }

    fn generate_ints(
        &mut self,
        area_x: i32,
        area_y: i32,
        area_width: i32,
        area_height: i32,
    ) -> Vec<i32> {
        let i = area_x - 1;
        let j = area_y - 1;
        let k = area_width + 2;
        let l = area_height + 2;
        let aint = parent_ints(self, i, j, k, l);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i1 in 0..area_height {
            for j1 in 0..area_width {
                let k1 = Self::filter(aint[(j1 + (i1 + 1) * k) as usize]);
                let l1 = Self::filter(aint[(j1 + 2 + (i1 + 1) * k) as usize]);
                let i2 = Self::filter(aint[(j1 + 1 + i1 * k) as usize]);
                let j2 = Self::filter(aint[(j1 + 1 + (i1 + 2) * k) as usize]);
                let k2 = Self::filter(aint[(j1 + 1 + (i1 + 1) * k) as usize]);

                aint1[(j1 + i1 * area_width) as usize] =
                    if k2 == k1 && k2 == i2 && k2 == l1 && k2 == j2 {
                        -1
                    } else {
                        7
                    };
            }
        }
        aint1
    }
}

/// 把河网叠回 biome 图。
pub struct RiverMix {
    base: LayerBase,
    biome_chain: Box<dyn Layer>,
    river_chain: Box<dyn Layer>,
}

impl RiverMix {
    pub fn new(seed: i64, biome_chain: Box<dyn Layer>, river_chain: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::new(seed),
            biome_chain,
            river_chain,
        }
    }
}

impl Layer for RiverMix {
    fn base(&self) -> &LayerBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut LayerBase {
        &mut self.base
    }

    /// PHP 里这一层没有 `parent`，而是自己把两条链都种一遍。
    fn init_world_gen_seed(&mut self, seed: i64) {
        self.biome_chain.init_world_gen_seed(seed);
        self.river_chain.init_world_gen_seed(seed);
        self.base.apply_seed(seed);
    }

    fn generate_ints(
        &mut self,
        area_x: i32,
        area_y: i32,
        area_width: i32,
        area_height: i32,
    ) -> Vec<i32> {
        let aint = self
            .biome_chain
            .get_ints(area_x, area_y, area_width, area_height);
        let aint1 = self
            .river_chain
            .get_ints(area_x, area_y, area_width, area_height);
        let mut aint2 = vec![0i32; (area_width * area_height) as usize];

        let total = (area_width * area_height) as usize;
        for i in 0..total {
            aint2[i] = if aint[i] != 0 && aint[i] != 24 {
                if aint1[i] == 7 {
                    if aint[i] == 12 {
                        11
                    } else if aint[i] != 14 && aint[i] != 15 {
                        aint1[i] & 255
                    } else {
                        15
                    }
                } else {
                    aint[i]
                }
            } else {
                aint[i]
            };
        }
        aint2
    }
}
