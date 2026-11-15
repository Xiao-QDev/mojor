//! `GenLayerIsland` / `GenLayerAddIsland` / `GenLayerRemoveTooMuchOcean` /
//! `GenLayerDeepOcean` / `GenLayerAddMushroomIsland`。
//!
//! 这几层的索引表达式在 PHP 里写得很难看（`$i1 + 1 - 1` 之类），
//! 但都是 `i1 * k` 的等价写法，照抄成简化形式即可，语义不变。

use crate::level::generator::realistic_terrain::biome::gen_layer::{Layer, LayerBase, parent_ints};

/// 起点层：随机撒陆地（10%）。
pub struct Island {
    base: LayerBase,
}

impl Island {
    pub fn new(seed: i64) -> Self {
        Self {
            base: LayerBase::new(seed),
        }
    }
}

impl Layer for Island {
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
        let mut aint = vec![0i32; (area_width * area_height) as usize];
        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(area_x + j, area_y + i);
                aint[(j + i * area_width) as usize] = if self.next_int(10) == 0 { 1 } else { 0 };
            }
        }

        // 原点附近强制一块陆地，保证出生点不是海
        if area_x > -area_width && area_x <= 0 && area_y > -area_height && area_y <= 0 {
            aint[(-area_x + -area_y * area_width) as usize] = 1;
        }
        aint
    }
}

/// 把陆地向外扩一圈。
pub struct AddIsland {
    base: LayerBase,
}

impl AddIsland {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for AddIsland {
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
                let k1 = aint[(j1 + i1 * k) as usize];
                let l1 = aint[(j1 + 2 + i1 * k) as usize];
                let i2 = aint[(j1 + (i1 + 2) * k) as usize];
                let j2 = aint[(j1 + 2 + (i1 + 2) * k) as usize];
                let k2 = aint[(j1 + 1 + (i1 + 1) * k) as usize];
                self.init_chunk_seed(j1 + area_x, i1 + area_y);

                let out = if k2 != 0 || k1 == 0 && l1 == 0 && i2 == 0 && j2 == 0 {
                    if k2 > 0 && (k1 == 0 || l1 == 0 || i2 == 0 || j2 == 0) {
                        if self.next_int(5) == 0 {
                            if k2 == 4 { 4 } else { 0 }
                        } else {
                            k2
                        }
                    } else {
                        k2
                    }
                } else {
                    let mut l2 = 1;
                    let mut i3 = 1;

                    // `$this->nextInt($l2++)` 在短路时不会执行，所以自增必须跟在取值之后
                    if k1 != 0 {
                        let hit = self.next_int(l2) == 0;
                        l2 += 1;
                        if hit {
                            i3 = k1;
                        }
                    }
                    if l1 != 0 {
                        let hit = self.next_int(l2) == 0;
                        l2 += 1;
                        if hit {
                            i3 = l1;
                        }
                    }
                    if i2 != 0 {
                        let hit = self.next_int(l2) == 0;
                        l2 += 1;
                        if hit {
                            i3 = i2;
                        }
                    }
                    if j2 != 0 {
                        let hit = self.next_int(l2) == 0;
                        l2 += 1;
                        if hit {
                            i3 = j2;
                        }
                    }
                    let _ = l2;

                    let r = self.next_int(3);
                    if r == 0 {
                        i3
                    } else if i3 == 4 {
                        4
                    } else {
                        0
                    }
                };

                aint1[(j1 + i1 * area_width) as usize] = out;
            }
        }
        aint1
    }
}

/// 四面全是海时，有 1/2 概率把孤立的海洋格填成陆地。
pub struct RemoveTooMuchOcean {
    base: LayerBase,
}

impl RemoveTooMuchOcean {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for RemoveTooMuchOcean {
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
                let k1 = aint[(j1 + 1 + i1 * k) as usize];
                let l1 = aint[(j1 + 2 + (i1 + 1) * k) as usize];
                let i2 = aint[(j1 + (i1 + 1) * k) as usize];
                let j2 = aint[(j1 + 1 + (i1 + 2) * k) as usize];
                let k2 = aint[(j1 + 1 + (i1 + 1) * k) as usize];
                aint1[(j1 + i1 * area_width) as usize] = k2;
                self.init_chunk_seed(j1 + area_x, i1 + area_y);

                if k2 == 0 && k1 == 0 && l1 == 0 && i2 == 0 && j2 == 0 && self.next_int(2) == 0 {
                    aint1[(j1 + i1 * area_width) as usize] = 1;
                }
            }
        }
        aint1
    }
}

/// 四面全海的中心格转成深海（24）。
pub struct DeepOcean {
    base: LayerBase,
}

impl DeepOcean {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for DeepOcean {
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
                let k1 = aint[(j1 + 1 + i1 * k) as usize];
                let l1 = aint[(j1 + 2 + (i1 + 1) * k) as usize];
                let i2 = aint[(j1 + (i1 + 1) * k) as usize];
                let j2 = aint[(j1 + 1 + (i1 + 2) * k) as usize];
                let k2 = aint[(j1 + 1 + (i1 + 1) * k) as usize];

                let mut l2 = 0;
                if k1 == 0 {
                    l2 += 1;
                }
                if l1 == 0 {
                    l2 += 1;
                }
                if i2 == 0 {
                    l2 += 1;
                }
                if j2 == 0 {
                    l2 += 1;
                }

                if k2 == 0 && l2 > 3 {
                    aint1[(j1 + i1 * area_width) as usize] = 24;
                } else {
                    aint1[(j1 + i1 * area_width) as usize] = k2;
                }
            }
        }
        aint1
    }
}

/// 全空的 3×3 中心有 1% 概率长蘑菇岛（14）。
pub struct AddMushroomIsland {
    base: LayerBase,
}

impl AddMushroomIsland {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for AddMushroomIsland {
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
                let k1 = aint[(j1 + i1 * k) as usize];
                let l1 = aint[(j1 + 2 + i1 * k) as usize];
                let i2 = aint[(j1 + (i1 + 2) * k) as usize];
                let j2 = aint[(j1 + 2 + (i1 + 2) * k) as usize];
                let k2 = aint[(j1 + 1 + (i1 + 1) * k) as usize];
                self.init_chunk_seed(j1 + area_x, i1 + area_y);

                aint1[(j1 + i1 * area_width) as usize] = if k2 == 0
                    && k1 == 0
                    && l1 == 0
                    && i2 == 0
                    && j2 == 0
                    && self.next_int(100) == 0
                {
                    14
                } else {
                    k2
                };
            }
        }
        aint1
    }
}
