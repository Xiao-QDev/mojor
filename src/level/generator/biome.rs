// 对齐 Nukkit 的 biome 包：群系表 地表覆盖 装饰器与温度降雨选择器
use crate::level::generator::blocks::*;
use crate::level::generator::noise::{Noise, NoiseKind};
use crate::level::generator::random::NukkitRandom;

pub const OCEAN: u8 = 0;
pub const PLAINS: u8 = 1;
pub const DESERT: u8 = 2;
pub const MOUNTAINS: u8 = 3;
pub const FOREST: u8 = 4;
pub const TAIGA: u8 = 5;
pub const SWAMP: u8 = 6;
pub const RIVER: u8 = 7;
pub const ICE_PLAINS: u8 = 12;
pub const SMALL_MOUNTAINS: u8 = 20;
pub const BIRCH_FOREST: u8 = 27;

// 每个群系在 populateChunk 阶段按顺序执行的装饰器
#[derive(Clone, Copy)]
pub enum PopulatorSpec {
    Tree {
        sapling: u8,
        base: i32,
        random: i32,
    },
    Grass { base: i32, random: i32 },
    TallGrass { base: i32, random: i32 },
    Flower { base: i32, random: i32, types: &'static [(u8, u8)] },
    Sugarcane { base: i32, random: i32 },
    TallSugarcane { base: i32, random: i32 },
    Cactus { base: i32, random: i32 },
    DeadBush { base: i32, random: i32 },
    LilyPad { base: i32, random: i32 },
}

pub struct BiomeDef {
    pub id: u8,
    pub name: &'static str,
    pub temperature: f64,
    pub rainfall: f64,
    // 草地颜色 低 24 位 RGB
    pub color: i32,
    pub ground_cover: &'static [u8],
    pub populators: &'static [PopulatorSpec],
}

// GrassyBiome：草 + 4 层泥土
static GRASSY: [u8; 5] = [GRASS, DIRT, DIRT, DIRT, DIRT];
// SandyBiome：沙 + 沙岩
static SANDY: [u8; 5] = [SAND, SAND, SANDSTONE, SANDSTONE, SANDSTONE];
// SnowyBiome：雪层下面才是草
static SNOWY: [u8; 5] = [SNOW_LAYER, GRASS, DIRT, DIRT, DIRT];
// WateryBiome：全是泥土
static WATERY: [u8; 5] = [DIRT, DIRT, DIRT, DIRT, DIRT];

static PLAINS_FLOWERS: [(u8, u8); 8] = [
    (DANDELION, 0),
    (RED_FLOWER, 0),
    (RED_FLOWER, 3),
    (RED_FLOWER, 4),
    (RED_FLOWER, 5),
    (RED_FLOWER, 6),
    (RED_FLOWER, 7),
    (RED_FLOWER, 8),
];

static SWAMP_FLOWERS: [(u8, u8); 1] = [(RED_FLOWER, 1)];

static OCEAN_DEF: BiomeDef = BiomeDef {
    id: OCEAN,
    name: "Ocean",
    temperature: 0.5,
    rainfall: 0.5,
    color: 0x8d_b871,
    ground_cover: &WATERY,
    populators: &[
        PopulatorSpec::Sugarcane { base: 6, random: 0 },
        PopulatorSpec::TallSugarcane { base: 60, random: 0 },
    ],
};

static PLAINS_DEF: BiomeDef = BiomeDef {
    id: PLAINS,
    name: "Plains",
    temperature: 0.8,
    rainfall: 0.4,
    color: 0x92_bc59,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Sugarcane { base: 6, random: 0 },
        PopulatorSpec::TallSugarcane { base: 60, random: 0 },
        PopulatorSpec::Grass { base: 40, random: 0 },
        PopulatorSpec::TallGrass { base: 7, random: 0 },
        PopulatorSpec::Flower {
            base: 10,
            random: 0,
            types: &PLAINS_FLOWERS,
        },
    ],
};

static DESERT_DEF: BiomeDef = BiomeDef {
    id: DESERT,
    name: "Desert",
    temperature: 2.0,
    rainfall: 0.0,
    color: 0xfc_b813,
    ground_cover: &SANDY,
    populators: &[
        PopulatorSpec::Cactus { base: 2, random: 0 },
        PopulatorSpec::DeadBush { base: 2, random: 0 },
    ],
};

static MOUNTAINS_DEF: BiomeDef = BiomeDef {
    id: MOUNTAINS,
    name: "Mountains",
    temperature: 0.4,
    rainfall: 0.5,
    color: 0x8d_b679,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Tree {
            sapling: SAPLING_OAK,
            base: 1,
            random: 0,
        },
        PopulatorSpec::Grass { base: 30, random: 0 },
        PopulatorSpec::TallGrass { base: 1, random: 0 },
    ],
};

static FOREST_DEF: BiomeDef = BiomeDef {
    id: FOREST,
    name: "Forest",
    temperature: 0.8,
    rainfall: 0.5,
    color: 0x8a_bd56,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Tree {
            sapling: SAPLING_OAK,
            base: 5,
            random: 0,
        },
        PopulatorSpec::Grass { base: 30, random: 0 },
        PopulatorSpec::TallGrass { base: 3, random: 0 },
    ],
};

static TAIGA_DEF: BiomeDef = BiomeDef {
    id: TAIGA,
    name: "Taiga",
    temperature: 0.05,
    rainfall: 0.8,
    color: 0x82_b493,
    ground_cover: &SNOWY,
    populators: &[
        PopulatorSpec::Grass { base: 6, random: 0 },
        PopulatorSpec::Tree {
            sapling: SAPLING_SPRUCE,
            base: 10,
            random: 0,
        },
        PopulatorSpec::TallGrass { base: 1, random: 0 },
    ],
};

static SWAMP_DEF: BiomeDef = BiomeDef {
    id: SWAMP,
    name: "Swamp",
    temperature: 0.8,
    rainfall: 0.9,
    // SwampBiome 直接覆写了 getColor
    color: 0x6a_7039,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Tree {
            sapling: SAPLING_OAK,
            base: 2,
            random: 0,
        },
        PopulatorSpec::Flower {
            base: 2,
            random: 0,
            types: &SWAMP_FLOWERS,
        },
        PopulatorSpec::LilyPad { base: 4, random: 0 },
    ],
};

static RIVER_DEF: BiomeDef = BiomeDef {
    id: RIVER,
    name: "River",
    temperature: 0.5,
    rainfall: 0.7,
    color: 0x86_b96f,
    ground_cover: &WATERY,
    populators: &[
        PopulatorSpec::Grass { base: 30, random: 0 },
        PopulatorSpec::TallGrass { base: 5, random: 0 },
        PopulatorSpec::Sugarcane { base: 6, random: 0 },
        PopulatorSpec::TallSugarcane { base: 60, random: 0 },
    ],
};

static ICE_PLAINS_DEF: BiomeDef = BiomeDef {
    id: ICE_PLAINS,
    name: "Ice Plains",
    temperature: 0.05,
    rainfall: 0.8,
    color: 0x82_b493,
    ground_cover: &GRASSY,
    populators: &[PopulatorSpec::TallGrass { base: 5, random: 0 }],
};

static SMALL_MOUNTAINS_DEF: BiomeDef = BiomeDef {
    id: SMALL_MOUNTAINS,
    name: "Small Mountains",
    temperature: 0.4,
    rainfall: 0.5,
    color: 0x8d_b679,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Tree {
            sapling: SAPLING_OAK,
            base: 1,
            random: 0,
        },
        PopulatorSpec::Grass { base: 30, random: 0 },
        PopulatorSpec::TallGrass { base: 1, random: 0 },
    ],
};

static BIRCH_FOREST_DEF: BiomeDef = BiomeDef {
    id: BIRCH_FOREST,
    name: "Birch Forest",
    temperature: 0.5,
    rainfall: 0.5,
    color: 0x8d_b871,
    ground_cover: &GRASSY,
    populators: &[
        PopulatorSpec::Tree {
            sapling: SAPLING_BIRCH,
            base: 5,
            random: 0,
        },
        PopulatorSpec::Grass { base: 30, random: 0 },
        PopulatorSpec::TallGrass { base: 3, random: 0 },
    ],
};

// 未知 id 回落为海洋 与 Biome.getBiome 一致
pub fn biome(id: u8) -> &'static BiomeDef {
    match id {
        PLAINS => &PLAINS_DEF,
        DESERT => &DESERT_DEF,
        MOUNTAINS => &MOUNTAINS_DEF,
        FOREST => &FOREST_DEF,
        TAIGA => &TAIGA_DEF,
        SWAMP => &SWAMP_DEF,
        RIVER => &RIVER_DEF,
        ICE_PLAINS => &ICE_PLAINS_DEF,
        SMALL_MOUNTAINS => &SMALL_MOUNTAINS_DEF,
        BIRCH_FOREST => &BIRCH_FOREST_DEF,
        _ => &OCEAN_DEF,
    }
}

// 温度降雨二维查表 对齐 BiomeSelector
#[derive(Clone)]
pub struct BiomeSelector {
    temperature: Noise,
    rainfall: Noise,
}

impl BiomeSelector {
    pub fn new(random: &mut NukkitRandom) -> Self {
        Self {
            temperature: Noise::new(NoiseKind::Simplex, random, 2.0, 1.0 / 8.0, 1.0 / 1024.0),
            rainfall: Noise::new(NoiseKind::Simplex, random, 2.0, 1.0 / 8.0, 1.0 / 1024.0),
        }
    }

    pub fn lookup(temperature: f64, rainfall: f64) -> u8 {
        if rainfall < 0.25 {
            SWAMP
        } else if rainfall < 0.60 {
            if temperature < 0.25 {
                ICE_PLAINS
            } else if temperature < 0.75 {
                PLAINS
            } else {
                DESERT
            }
        } else if rainfall < 0.80 {
            if temperature < 0.25 {
                TAIGA
            } else if temperature < 0.75 {
                FOREST
            } else {
                BIRCH_FOREST
            }
        } else if temperature < 0.25 {
            MOUNTAINS
        } else {
            SMALL_MOUNTAINS
        }
    }

    pub fn temperature_at(&self, x: f64, z: f64) -> f64 {
        (self.temperature.noise2d(x, z, true) + 1.0) / 2.0
    }

    pub fn rainfall_at(&self, x: f64, z: f64) -> f64 {
        (self.rainfall.noise2d(x, z, true) + 1.0) / 2.0
    }

    pub fn pick(&self, x: f64, z: f64) -> u8 {
        let temperature = (self.temperature_at(x, z) * 63.0) as i32;
        let rainfall = (self.rainfall_at(x, z) * 63.0) as i32;
        Self::lookup(temperature as f64 / 63.0, rainfall as f64 / 63.0)
    }
}
