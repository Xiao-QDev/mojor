//! `GenLayerBiome` / `GenLayerBiomeEdge` / `GenLayerHills` /
//! `GenLayerRareBiome` / `GenLayerShore`。

use crate::level::generator::realistic_terrain::biome::gen_layer::{
    Layer, LayerBase, is_biome_oceanic, parent_ints,
};
use crate::level::generator::realistic_terrain::biome::table::{
    BiomeClass, TempCategory, biome_class, biomes_equal_or_mesa_plateau, get_biome, id_of,
    is_mutation, is_snowy, mutation_for, temp_category,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GeneratorType {
    Default,
    LargeBiomes,
    Default11,
}

#[derive(Clone, Copy, Debug)]
pub struct GenSettings {
    pub biome_size: i32,
    pub river_size: i32,
    pub fixed_biome: i32,
}

/// 把温带码（1/2/3/4）翻译成具体 biome。
pub struct BiomeLayer {
    base: LayerBase,
    warm: &'static [i32],
    medium: &'static [i32],
    cold: &'static [i32],
    ice: &'static [i32],
    fixed_biome: Option<i32>,
}

const WARM: &[i32] = &[2, 2, 2, 35, 35, 1];
const MEDIUM: &[i32] = &[4, 29, 3, 1, 27, 6];
const COLD: &[i32] = &[4, 3, 5, 1];
const ICE: &[i32] = &[12, 12, 12, 30];
const WARM_11: &[i32] = &[2, 4, 3, 6, 1, 5];

impl BiomeLayer {
    pub fn new(
        seed: i64,
        parent: Box<dyn Layer>,
        kind: GeneratorType,
        settings: Option<GenSettings>,
    ) -> Self {
        let (warm, fixed_biome) = if kind == GeneratorType::Default11 {
            (WARM_11, None)
        } else {
            (
                WARM,
                settings.and_then(|s| {
                    if s.fixed_biome >= 0 {
                        Some(s.fixed_biome)
                    } else {
                        None
                    }
                }),
            )
        };
        Self {
            base: LayerBase::with_parent(seed, parent),
            warm,
            medium: MEDIUM,
            cold: COLD,
            ice: ICE,
            fixed_biome,
        }
    }
}

impl Layer for BiomeLayer {
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
                let raw = aint[(j + i * area_width) as usize];
                let l = (raw & 3840) >> 8;
                let k = raw & !3840;

                let out = if let Some(fixed) = self.fixed_biome {
                    fixed
                } else if is_biome_oceanic(k) {
                    k
                } else if k == 14 {
                    k
                } else if k == 1 {
                    if l > 0 {
                        if self.next_int(3) == 0 { 39 } else { 38 }
                    } else {
                        let n = self.next_int(self.warm.len() as i32) as usize;
                        self.warm[n]
                    }
                } else if k == 2 {
                    if l > 0 {
                        21
                    } else {
                        let n = self.next_int(self.medium.len() as i32) as usize;
                        self.medium[n]
                    }
                } else if k == 3 {
                    if l > 0 {
                        32
                    } else {
                        let n = self.next_int(self.cold.len() as i32) as usize;
                        self.cold[n]
                    }
                } else if k == 4 {
                    let n = self.next_int(self.ice.len() as i32) as usize;
                    self.ice[n]
                } else {
                    14
                };

                aint1[(j + i * area_width) as usize] = out;
            }
        }
        aint1
    }
}

/// 修饰 biome 交界。
pub struct BiomeEdge {
    base: LayerBase,
}

impl BiomeEdge {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_if_necessary(
        &self,
        aint: &[i32],
        out: &mut [i32],
        j: i32,
        i: i32,
        width: i32,
        biome: i32,
        replaced: i32,
        fallback: i32,
    ) -> bool {
        if !biomes_equal_or_mesa_plateau(biome, replaced) {
            return false;
        }
        let k = width + 2;
        let n = aint[(j + 1 + i * k) as usize];
        let e = aint[(j + 2 + (i + 1) * k) as usize];
        let w = aint[(j + (i + 1) * k) as usize];
        let s = aint[(j + 1 + (i + 2) * k) as usize];

        out[(j + i * width) as usize] = if can_biomes_be_neighbors(n, replaced)
            && can_biomes_be_neighbors(e, replaced)
            && can_biomes_be_neighbors(w, replaced)
            && can_biomes_be_neighbors(s, replaced)
        {
            biome
        } else {
            fallback
        };
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_edge(
        &self,
        aint: &[i32],
        out: &mut [i32],
        j: i32,
        i: i32,
        width: i32,
        biome: i32,
        replaced: i32,
        fallback: i32,
    ) -> bool {
        if biome != replaced {
            return false;
        }
        let k = width + 2;
        let n = aint[(j + 1 + i * k) as usize];
        let e = aint[(j + 2 + (i + 1) * k) as usize];
        let w = aint[(j + (i + 1) * k) as usize];
        let s = aint[(j + 1 + (i + 2) * k) as usize];

        out[(j + i * width) as usize] = if biomes_equal_or_mesa_plateau(n, replaced)
            && biomes_equal_or_mesa_plateau(e, replaced)
            && biomes_equal_or_mesa_plateau(w, replaced)
            && biomes_equal_or_mesa_plateau(s, replaced)
        {
            biome
        } else {
            fallback
        };
        true
    }
}

fn can_biomes_be_neighbors(a: i32, b: i32) -> bool {
    if biomes_equal_or_mesa_plateau(a, b) {
        return true;
    }
    match (temp_category(a), temp_category(b)) {
        (Some(ta), Some(tb)) => {
            ta == tb || ta == TempCategory::Medium || tb == TempCategory::Medium
        }
        _ => false,
    }
}

impl Layer for BiomeEdge {
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
        let k = area_width + 2;
        let aint = parent_ints(self, area_x - 1, area_y - 1, k, area_height + 2);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                let biome = aint[(j + 1 + (i + 1) * k) as usize];

                // 四段短路求值必须保持顺序：每一段都可能写 aint1
                let mut handled =
                    self.replace_if_necessary(&aint, &mut aint1, j, i, area_width, biome, 3, 20);
                if !handled {
                    handled = self.replace_edge(&aint, &mut aint1, j, i, area_width, biome, 38, 37);
                }
                if !handled {
                    handled = self.replace_edge(&aint, &mut aint1, j, i, area_width, biome, 39, 37);
                }
                if !handled {
                    handled = self.replace_edge(&aint, &mut aint1, j, i, area_width, biome, 32, 5);
                }

                if !handled {
                    let out = if biome == 2 {
                        let n = aint[(j + 1 + i * k) as usize];
                        let e = aint[(j + 2 + (i + 1) * k) as usize];
                        let w = aint[(j + (i + 1) * k) as usize];
                        let s = aint[(j + 1 + (i + 2) * k) as usize];
                        if n != 12 && e != 12 && w != 12 && s != 12 {
                            biome
                        } else {
                            34
                        }
                    } else if biome == 6 {
                        let n = aint[(j + 1 + i * k) as usize];
                        let e = aint[(j + 2 + (i + 1) * k) as usize];
                        let w = aint[(j + (i + 1) * k) as usize];
                        let s = aint[(j + 1 + (i + 2) * k) as usize];
                        if n != 2
                            && e != 2
                            && w != 2
                            && s != 2
                            && n != 30
                            && e != 30
                            && w != 30
                            && s != 30
                            && n != 12
                            && e != 12
                            && w != 12
                            && s != 12
                        {
                            if n != 21 && s != 21 && e != 21 && w != 21 {
                                biome
                            } else {
                                23
                            }
                        } else {
                            1
                        }
                    } else {
                        biome
                    };
                    aint1[(j + i * area_width) as usize] = out;
                }
            }
        }
        aint1
    }
}

/// 丘陵：按河流值决定是否把平坦 biome 换成带山的变体。
pub struct Hills {
    base: LayerBase,
    river_layer: Box<dyn Layer>,
}

impl Hills {
    pub fn new(seed: i64, parent: Box<dyn Layer>, river_layer: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
            river_layer,
        }
    }
}

impl Layer for Hills {
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
        let k = area_width + 2;
        let aint = parent_ints(self, area_x - 1, area_y - 1, k, area_height + 2);
        // 注意：PHP 从未对 riverLayer 调过 initWorldGenSeed，
        // 它的 worldGenSeed 一直是 0。这里照旧，不要补种。
        // 注意：丘陵这条河流支路上的两层 `Zoom` 从没被播种过（`worldGenSeed` 一直是 0），
        // 只有它们下面的 `RiverInit` 因为与主河流支链共享对象、被
        // `RiverMix::initWorldGenSeed` 连带播了种。这里照旧，不要补种。
        let aint1 = self
            .river_layer
            .get_ints(area_x - 1, area_y - 1, k, area_height + 2);
        let mut aint2 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                let biome_id = aint[(j + 1 + (i + 1) * k) as usize];
                let l = aint1[(j + 1 + (i + 1) * k) as usize];
                let flag = (l - 2) % 29 == 0;

                let biome = get_biome(biome_id);
                let flag1 = biome.map_or(false, is_mutation);

                let out = if biome_id != 0 && l >= 2 && (l - 2) % 29 == 1 && !flag1 {
                    mutation_for(biome).unwrap_or(biome_id)
                } else if self.next_int(3) != 0 && !flag {
                    biome_id
                } else {
                    let mut biome1 = biome;
                    if biome == Some(2) {
                        biome1 = Some(17);
                    } else if biome == Some(4) {
                        biome1 = Some(18);
                    } else if biome == Some(27) {
                        biome1 = Some(28);
                    } else if biome == Some(29) {
                        biome1 = Some(1);
                    } else if biome == Some(5) {
                        biome1 = Some(19);
                    } else if biome == Some(32) {
                        biome1 = Some(33);
                    } else if biome == Some(30) {
                        biome1 = Some(31);
                    } else if biome == Some(1) {
                        biome1 = if self.next_int(3) == 0 {
                            Some(18)
                        } else {
                            Some(4)
                        };
                    } else if biome == Some(12) {
                        biome1 = Some(13);
                    } else if biome == Some(21) {
                        biome1 = Some(22);
                    } else if biome == Some(0) {
                        biome1 = Some(24);
                    } else if biome == Some(3) {
                        biome1 = Some(34);
                    } else if biome == Some(35) {
                        biome1 = Some(36);
                    } else if biomes_equal_or_mesa_plateau(biome_id, 38) {
                        biome1 = Some(37);
                    } else if biome == Some(24) && self.next_int(3) == 0 {
                        biome1 = if self.next_int(2) == 0 {
                            Some(1)
                        } else {
                            Some(4)
                        };
                    }

                    let mut j2 = id_of(biome1);

                    if flag && j2 != biome_id {
                        j2 = mutation_for(biome1).unwrap_or(biome_id);
                    }

                    if j2 == biome_id {
                        biome_id
                    } else {
                        let n = aint[(j + 1 + i * k) as usize];
                        let e = aint[(j + 2 + (i + 1) * k) as usize];
                        let w = aint[(j + (i + 1) * k) as usize];
                        let s = aint[(j + 1 + (i + 2) * k) as usize];
                        let mut same = 0;
                        if biomes_equal_or_mesa_plateau(n, biome_id) {
                            same += 1;
                        }
                        if biomes_equal_or_mesa_plateau(e, biome_id) {
                            same += 1;
                        }
                        if biomes_equal_or_mesa_plateau(w, biome_id) {
                            same += 1;
                        }
                        if biomes_equal_or_mesa_plateau(s, biome_id) {
                            same += 1;
                        }
                        if same >= 3 { j2 } else { biome_id }
                    }
                };

                aint2[(j + i * area_width) as usize] = out;
            }
        }
        aint2
    }
}

/// 每 57 格把平原换成向日葵平原。
pub struct RareBiome {
    base: LayerBase,
}

impl RareBiome {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }
}

impl Layer for RareBiome {
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
        let k = area_width + 2;
        let aint = parent_ints(self, area_x - 1, area_y - 1, k, area_height + 2);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                let biome = aint[(j + 1 + (i + 1) * k) as usize];

                aint1[(j + i * area_width) as usize] = if self.next_int(57) == 0 && biome == 1 {
                    129
                } else {
                    biome
                };
            }
        }
        aint1
    }
}

/// 海岸线：把贴着海的陆地块换成沙滩/石岸。
pub struct Shore {
    base: LayerBase,
}

impl Shore {
    pub fn new(seed: i64, parent: Box<dyn Layer>) -> Self {
        Self {
            base: LayerBase::with_parent(seed, parent),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn replace_if_neighbor_ocean(
        &self,
        aint: &[i32],
        out: &mut [i32],
        j: i32,
        i: i32,
        width: i32,
        biome: i32,
        fallback: i32,
    ) {
        if is_biome_oceanic(biome) {
            out[(j + i * width) as usize] = biome;
            return;
        }
        let k = width + 2;
        let n = aint[(j + 1 + i * k) as usize];
        let e = aint[(j + 2 + (i + 1) * k) as usize];
        let w = aint[(j + (i + 1) * k) as usize];
        let s = aint[(j + 1 + (i + 2) * k) as usize];

        out[(j + i * width) as usize] = if !is_biome_oceanic(n)
            && !is_biome_oceanic(e)
            && !is_biome_oceanic(w)
            && !is_biome_oceanic(s)
        {
            biome
        } else {
            fallback
        };
    }
}

fn is_jungle_compatible(id: i32) -> bool {
    if biome_class(id) == Some(BiomeClass::Jungle) {
        return true;
    }
    id == 23 || id == 21 || id == 22 || id == 4 || id == 5 || is_biome_oceanic(id)
}

fn is_mesa(id: i32) -> bool {
    biome_class(id) == Some(BiomeClass::Mesa)
}

impl Layer for Shore {
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
        let k = area_width + 2;
        let aint = parent_ints(self, area_x - 1, area_y - 1, k, area_height + 2);
        let mut aint1 = vec![0i32; (area_width * area_height) as usize];

        for i in 0..area_height {
            for j in 0..area_width {
                self.init_chunk_seed(j + area_x, i + area_y);
                let biome_id = aint[(j + 1 + (i + 1) * k) as usize];

                if biome_id == 14 {
                    let n = aint[(j + 1 + i * k) as usize];
                    let e = aint[(j + 2 + (i + 1) * k) as usize];
                    let w = aint[(j + (i + 1) * k) as usize];
                    let s = aint[(j + 1 + (i + 2) * k) as usize];
                    aint1[(j + i * area_width) as usize] = if n != 0 && e != 0 && w != 0 && s != 0 {
                        biome_id
                    } else {
                        15
                    };
                } else if biome_class(biome_id) == Some(BiomeClass::Jungle) {
                    let n = aint[(j + 1 + i * k) as usize];
                    let e = aint[(j + 2 + (i + 1) * k) as usize];
                    let w = aint[(j + (i + 1) * k) as usize];
                    let s = aint[(j + 1 + (i + 2) * k) as usize];
                    if is_jungle_compatible(n)
                        && is_jungle_compatible(e)
                        && is_jungle_compatible(w)
                        && is_jungle_compatible(s)
                    {
                        aint1[(j + i * area_width) as usize] = if !is_biome_oceanic(n)
                            && !is_biome_oceanic(e)
                            && !is_biome_oceanic(w)
                            && !is_biome_oceanic(s)
                        {
                            biome_id
                        } else {
                            16
                        };
                    } else {
                        aint1[(j + i * area_width) as usize] = 23;
                    }
                } else if biome_id != 3 && biome_id != 34 && biome_id != 20 {
                    if is_snowy(biome_id) {
                        self.replace_if_neighbor_ocean(
                            &aint, &mut aint1, j, i, area_width, biome_id, 26,
                        );
                    } else if biome_id != 37 && biome_id != 38 {
                        if biome_id != 0 && biome_id != 24 && biome_id != 7 && biome_id != 6 {
                            let n = aint[(j + 1 + i * k) as usize];
                            let e = aint[(j + 2 + (i + 1) * k) as usize];
                            let w = aint[(j + (i + 1) * k) as usize];
                            let s = aint[(j + 1 + (i + 2) * k) as usize];
                            aint1[(j + i * area_width) as usize] = if !is_biome_oceanic(n)
                                && !is_biome_oceanic(e)
                                && !is_biome_oceanic(w)
                                && !is_biome_oceanic(s)
                            {
                                biome_id
                            } else {
                                16
                            };
                        } else {
                            aint1[(j + i * area_width) as usize] = biome_id;
                        }
                    } else {
                        let n = aint[(j + 1 + i * k) as usize];
                        let e = aint[(j + 2 + (i + 1) * k) as usize];
                        let w = aint[(j + (i + 1) * k) as usize];
                        let s = aint[(j + 1 + (i + 2) * k) as usize];
                        aint1[(j + i * area_width) as usize] = if !is_biome_oceanic(n)
                            && !is_biome_oceanic(e)
                            && !is_biome_oceanic(w)
                            && !is_biome_oceanic(s)
                        {
                            if is_mesa(n) && is_mesa(e) && is_mesa(w) && is_mesa(s) {
                                biome_id
                            } else {
                                2
                            }
                        } else {
                            biome_id
                        };
                    }
                } else {
                    self.replace_if_neighbor_ocean(
                        &aint, &mut aint1, j, i, area_width, biome_id, 25,
                    );
                }
            }
        }
        aint1
    }
}
