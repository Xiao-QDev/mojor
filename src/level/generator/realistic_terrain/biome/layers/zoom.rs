//! `GenLayerZoom` / `GenLayerFuzzyZoom` / `GenLayerSmooth` / `GenLayerVoronoiZoom`。

use crate::level::generator::realistic_terrain::biome::gen_layer::{Layer, LayerBase, parent_ints};

/// 逐格放大 2 倍。`fuzzy` 为真时对应 `GenLayerFuzzyZoom`：
/// 四个值不再取众数，而是纯随机（PHP 只重写了 `selectModeOrRandom`）。
pub struct Zoom {
    base: LayerBase,
    fuzzy: bool,
}

impl Zoom {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
            fuzzy: false,
        }
    }

    pub fn fuzzy(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
            fuzzy: true,
        }
    }
}

impl Layer for Zoom {
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
        let i = area_x >> 1;
        let j = area_y >> 1;
        let k = (area_width >> 1) + 2;
        let l = (area_height >> 1) + 2;
        let aint = parent_ints(self, i, j, k, l);

        let i1 = (k - 1) << 1;
        let j1 = (l - 1) << 1;
        let mut aint1 = vec![0i32; (i1 * j1) as usize];

        for k1 in 0..l - 1 {
            let mut l1 = (k1 << 1) * i1;
            let mut i2 = 0;
            let mut j2 = aint[(i2 + (k1) * k) as usize];
            let mut k2 = aint[(i2 + (k1 + 1) * k) as usize];

            while i2 < k - 1 {
                let same_right = aint[(i2 + 1 + k1 * k) as usize];
                let same_below = aint[(i2 + 1 + (k1 + 1) * k) as usize];

                if j2 == k2 && j2 == same_right && j2 == same_below {
                    aint1[l1 as usize] = j2;
                    aint1[(l1 + i1) as usize] = j2;
                    l1 += 1;
                    aint1[l1 as usize] = j2;
                    aint1[(l1 + i1) as usize] = j2;
                    l1 += 1;
                    i2 += 1;
                    continue;
                }

                self.init_chunk_seed((i2 + i) << 1, (k1 + j) << 1);
                let l2 = aint[(i2 + 1 + k1 * k) as usize];
                let i3 = aint[(i2 + 1 + (k1 + 1) * k) as usize];

                aint1[l1 as usize] = j2;
                aint1[(l1 + i1) as usize] = self.select_random(&[j2, k2]);
                l1 += 1;
                aint1[l1 as usize] = self.select_random(&[j2, l2]);
                aint1[(l1 + i1) as usize] = if self.fuzzy {
                    self.select_random(&[j2, l2, k2, i3])
                } else {
                    self.select_mode_or_random(j2, l2, k2, i3)
                };
                l1 += 1;

                j2 = l2;
                k2 = i3;
                i2 += 1;
            }
        }

        let mut aint2 = vec![0i32; (area_width * area_height) as usize];
        for j3 in 0..area_height {
            for copy_index in 0..area_width {
                aint2[(j3 * area_width + copy_index) as usize] =
                    aint1[((j3 + (area_y & 1)) * i1 + (area_x & 1) + copy_index) as usize];
            }
        }
        aint2
    }
}

/// `GenLayerZoom::magnify`：连续套 `count` 层 Zoom，种子逐层 +1。
pub fn magnify(seed: i64, mut parent: Box<dyn Layer>, count: i32) -> Box<dyn Layer> {
    for i in 0..count {
        parent = Box::new(Zoom::new(seed + i as i64, parent));
    }
    parent
}

/// `GenLayerSmooth`：四邻取值，把单格噪声抹平。
pub struct Smooth {
    base: LayerBase,
}

impl Smooth {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for Smooth {
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
                let k1 = aint[(j1 + (i1 + 1) * k) as usize];
                let l1 = aint[(j1 + 2 + (i1 + 1) * k) as usize];
                let i2 = aint[(j1 + 1 + i1 * k) as usize];
                let j2 = aint[(j1 + 1 + (i1 + 2) * k) as usize];
                let mut k2 = aint[(j1 + 1 + (i1 + 1) * k) as usize];

                if k1 == l1 && i2 == j2 {
                    self.init_chunk_seed(j1 + area_x, i1 + area_y);
                    k2 = if self.next_int(2) == 0 { k1 } else { i2 };
                } else {
                    if k1 == l1 {
                        k2 = k1;
                    }
                    if i2 == j2 {
                        k2 = i2;
                    }
                }

                aint1[(j1 + i1 * area_width) as usize] = k2;
            }
        }
        aint1
    }
}

/// `GenLayerVoronoiZoom`：最后一步，按 4×4 的 Voronoi 单元给边缘加不规则边界。
pub struct VoronoiZoom {
    base: LayerBase,
}

impl VoronoiZoom {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }

    fn jitter(&mut self) -> f64 {
        (self.next_int(1024) as f64 / 1024.0 - 0.5) * 3.6
    }
}

impl Layer for VoronoiZoom {
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
        let area_x = area_x - 2;
        let area_y = area_y - 2;
        let i = area_x >> 2;
        let j = area_y >> 2;
        let k = (area_width >> 2) + 2;
        let l = (area_height >> 2) + 2;
        let aint = parent_ints(self, i, j, k, l);

        let i1 = (k - 1) << 2;
        let j1 = (l - 1) << 2;
        let mut aint1 = vec![0i32; (i1 * j1) as usize];

        for k1 in 0..l - 1 {
            let mut l1 = 0;
            let mut i2 = aint[(l1 + (k1) * k) as usize];
            let mut j2 = aint[(l1 + (k1 + 1) * k) as usize];

            while l1 < k - 1 {
                let same_right = aint[(l1 + 1 + k1 * k) as usize] & 255;
                let same_below = aint[(l1 + 1 + (k1 + 1) * k) as usize] & 255;

                if i2 == j2 && i2 == same_right && i2 == same_below {
                    for row in 0..4 {
                        let at = ((k1 << 2) + row) * i1 + (l1 << 2);
                        aint1[at as usize] = i2;
                        aint1[(at + 1) as usize] = i2;
                        aint1[(at + 2) as usize] = i2;
                        aint1[(at + 3) as usize] = i2;
                    }
                    l1 += 1;
                    continue;
                }

                self.init_chunk_seed((l1 + i) << 2, (k1 + j) << 2);
                let d1 = self.jitter();
                let d2 = self.jitter();
                self.init_chunk_seed((l1 + i + 1) << 2, (k1 + j) << 2);
                let d3 = self.jitter() + 4.0;
                let d4 = self.jitter();
                self.init_chunk_seed((l1 + i) << 2, (k1 + j + 1) << 2);
                let d5 = self.jitter();
                let d6 = self.jitter() + 4.0;
                self.init_chunk_seed((l1 + i + 1) << 2, (k1 + j + 1) << 2);
                let d7 = self.jitter() + 4.0;
                let d8 = self.jitter() + 4.0;

                let k2 = aint[(l1 + 1 + k1 * k) as usize] & 255;
                let l2 = aint[(l1 + 1 + (k1 + 1) * k) as usize] & 255;

                for i3 in 0..4 {
                    let mut j3 = ((k1 << 2) + i3) * i1 + (l1 << 2);
                    for k3 in 0..4 {
                        let f3 = i3 as f64;
                        let f4 = k3 as f64;
                        let d9 = (f3 - d2) * (f3 - d2) + (f4 - d1) * (f4 - d1);
                        let d10 = (f3 - d4) * (f3 - d4) + (f4 - d3) * (f4 - d3);
                        let d11 = (f3 - d6) * (f3 - d6) + (f4 - d5) * (f4 - d5);
                        let d12 = (f3 - d8) * (f3 - d8) + (f4 - d7) * (f4 - d7);

                        aint1[j3 as usize] = if d9 < d10 && d9 < d11 && d9 < d12 {
                            i2
                        } else if d10 < d9 && d10 < d11 && d10 < d12 {
                            k2
                        } else if d11 < d9 && d11 < d10 && d11 < d12 {
                            j2
                        } else {
                            l2
                        };
                        j3 += 1;
                    }
                }

                i2 = k2;
                j2 = l2;
                l1 += 1;
            }
        }

        let mut aint2 = vec![0i32; (area_width * area_height) as usize];
        for l3 in 0..area_height {
            for copy_index in 0..area_width {
                aint2[(l3 * area_width + copy_index) as usize] =
                    aint1[((l3 + (area_y & 3)) * i1 + (area_x & 3) + copy_index) as usize];
            }
        }
        aint2
    }
}
