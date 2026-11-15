//! `GenLayerAddSnow` / `GenLayerEdge`。

use crate::level::generator::realistic_terrain::biome::gen_layer::{Layer, LayerBase, parent_ints};

/// 给陆地随机分配温带/寒带/雪原标记（1/3/4）。
pub struct AddSnow {
    base: LayerBase,
}

impl AddSnow {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for AddSnow {
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
                let k1 = aint[(j1 + 1 + (i1 + 1) * k) as usize];
                self.init_chunk_seed(j1 + area_x, i1 + area_y);

                aint1[(j1 + i1 * area_width) as usize] = if k1 == 0 {
                    0
                } else {
                    let l1 = self.next_int(6);
                    if l1 == 0 {
                        4
                    } else if l1 <= 1 {
                        3
                    } else {
                        1
                    }
                };
            }
        }
        aint1
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EdgeMode {
    CoolWarm,
    HeatIce,
    Special,
}

/// 气候带交界：把相邻的温带/寒带衔接成中间带。
pub struct Edge {
    base: LayerBase,
    mode: EdgeMode,
}

impl Edge {
    pub fn new(seed: i64, parent: Box<dyn Layer>, mode: EdgeMode) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
            mode,
        }
    }
}

impl Layer for Edge {
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
        match self.mode {
            EdgeMode::CoolWarm => self.cool_warm(area_x, area_y, area_width, area_height),
            EdgeMode::HeatIce => self.heat_ice(area_x, area_y, area_width, area_height),
            EdgeMode::Special => self.special(area_x, area_y, area_width, area_height),
        }
    }
}

impl Edge {
    fn cool_warm(
        &mut self,
        area_x: i32,
        area_y: i32,
        area_width: i32,
        area_height: i32,
    ) -> Vec<i32> {
        let i = area_x - 1;
        let j = area_y - 1;
        let k = 1 + area_width + 1;
        let l = 1 + area_height + 1;
        let aint = parent_ints(self, i, j, k, l);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i1 in 0..area_height {
            for j1 in 0..area_width {
                self.init_chunk_seed(j1 + area_x, i1 + area_y);
                let mut k1 = aint[(j1 + 1 + (i1 + 1) * k) as usize];

                if k1 == 1 {
                    let l1 = aint[(j1 + 1 + i1 * k) as usize];
                    let i2 = aint[(j1 + 2 + (i1 + 1) * k) as usize];
                    let j2 = aint[(j1 + (i1 + 1) * k) as usize];
                    let k2 = aint[(j1 + 1 + (i1 + 2) * k) as usize];
                    let flag = l1 == 3 || i2 == 3 || j2 == 3 || k2 == 3;
                    let flag1 = l1 == 4 || i2 == 4 || j2 == 4 || k2 == 4;

                    if flag || flag1 {
                        k1 = 2;
                    }
                }

                aint1[(j1 + i1 * area_width) as usize] = k1;
            }
        }
        aint1
    }

    fn heat_ice(
        &mut self,
        area_x: i32,
        area_y: i32,
        area_width: i32,
        area_height: i32,
    ) -> Vec<i32> {
        let i = area_x - 1;
        let j = area_y - 1;
        let k = 1 + area_width + 1;
        let l = 1 + area_height + 1;
        let aint = parent_ints(self, i, j, k, l);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i1 in 0..area_height {
            for j1 in 0..area_width {
                let mut k1 = aint[(j1 + 1 + (i1 + 1) * k) as usize];

                if k1 == 4 {
                    let l1 = aint[(j1 + 1 + i1 * k) as usize];
                    let i2 = aint[(j1 + 2 + (i1 + 1) * k) as usize];
                    let j2 = aint[(j1 + (i1 + 1) * k) as usize];
                    let k2 = aint[(j1 + 1 + (i1 + 2) * k) as usize];
                    let flag = l1 == 2 || i2 == 2 || j2 == 2 || k2 == 2;
                    let flag1 = l1 == 1 || i2 == 1 || j2 == 1 || k2 == 1;

                    if flag1 || flag {
                        k1 = 3;
                    }
                }

                aint1[(j1 + i1 * area_width) as usize] = k1;
            }
        }
        aint1
    }

    fn special(&mut self, area_x: i32, area_y: i32, area_width: i32, area_height: i32) -> Vec<i32> {
        let aint = parent_ints(self, area_x, area_y, area_width, area_height);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                let mut k = aint[(j + i * area_width) as usize];

                if k != 0 && self.next_int(13) == 0 {
                    // PHP 运算符优先级：(1 + nextInt(15)) << 8，再 & 3840
                    k |= ((1 + self.next_int(15)) << 8) & 3840;
                }

                aint1[(j + i * area_width) as usize] = k;
            }
        }
        aint1
    }
}
