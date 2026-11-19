//! `BiomeDecorator::decorate` 与各 biome 的 decorate / getTreeFeature / getGrassFeature /
//! getRandomFlowerTypeAndData 覆写（MCPE 0.14.1 `biome/*.cpp`）。

use super::features::{self as f, Src, Tree};
use crate::level::generator::mcpe_noise::{McpeRandom, PerlinSimplexNoise};

/// 一个 biome 的 BiomeDecorator 计数器
#[derive(Clone, Copy, Debug)]
pub struct Counters {
    pub waterlily: i32,
    pub tree_density: f32,
    pub flowers: i32,
    pub grass: i32,
    pub dead_bush: i32,
    pub mushrooms: i32,
    pub reeds: i32,
    pub cactus: i32,
    pub gravel: i32,
    pub sand: i32,
    pub clay: i32,
    pub huge_mushroom: i32,
}

const DEFAULT: Counters = Counters {
    waterlily: 0,
    tree_density: 0.0,
    flowers: 2,
    grass: 1,
    dead_bush: 0,
    mushrooms: 0,
    reeds: 0,
    cactus: 0,
    gravel: 1,
    sand: 3,
    clay: 1,
    huge_mushroom: 0,
};

/// biome 类及构造参数
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Plain,
    Plains { mutated: bool },
    Desert,
    ExtremeHills,
    Forest(u8),
    Taiga(u8),
    Swamp,
    Ice { spikes: bool },
    MushroomIsland,
    Jungle { big: bool },
    Savanna,
    Mesa,
    /// MutatedBiome（含 MutatedSavannaBiome：decorate 走基础 biome 的装饰器）
    Mutated(u8),
    MutatedBirch(u8),
}

/// `Biome::initBiomes` 的 id → (类, 自己的计数器)
pub fn biome_info(id: u8) -> (Kind, Counters) {
    let mut c = DEFAULT;
    let kind = match id {
        1 | 129 => {
            c.tree_density = 0.05;
            c.flowers = 4;
            c.grass = 10;
            Kind::Plains { mutated: id == 129 }
        }
        2 | 17 => {
            c.tree_density = -999.0;
            c.dead_bush = 2;
            c.reeds = 50;
            c.cactus = 10;
            Kind::Desert
        }
        3 | 131 | 162 => Kind::ExtremeHills,
        20 | 34 => {
            c.tree_density = 3.0;
            Kind::ExtremeHills
        }
        4 | 18 | 27 | 28 => {
            c.tree_density = 10.0;
            c.grass = 0;
            Kind::Forest(if id >= 27 { 2 } else { 0 })
        }
        132 => {
            c.tree_density = 1.0;
            c.flowers = 100;
            c.grass = 1;
            Kind::Forest(1)
        }
        29 => {
            c.tree_density = -999.0;
            c.grass = 0;
            Kind::Forest(3)
        }
        5 | 19 | 30 | 31 => {
            c.tree_density = 10.0;
            c.grass = 1;
            c.mushrooms = 1;
            Kind::Taiga(0)
        }
        32 | 33 | 160 => {
            c.tree_density = 10.0;
            c.grass = 7;
            c.dead_bush = 1;
            c.mushrooms = 3;
            Kind::Taiga(if id == 160 { 2 } else { 1 })
        }
        6 => {
            c.mushrooms = 8;
            c.reeds = 10;
            c.waterlily = 4;
            c.grass = 5;
            c.tree_density = 2.0;
            c.flowers = 1;
            c.sand = 0;
            c.gravel = 0;
            c.dead_bush = 1;
            c.clay = 1;
            Kind::Swamp
        }
        12 | 13 => Kind::Ice { spikes: false },
        140 => Kind::Ice { spikes: true },
        14 | 15 => {
            c.tree_density = -100.0;
            c.flowers = -100;
            c.grass = -100;
            c.mushrooms = 1;
            c.huge_mushroom = 1;
            Kind::MushroomIsland
        }
        16 | 26 | 25 => {
            c.tree_density = -999.0;
            c.dead_bush = 0;
            c.reeds = 0;
            c.cactus = 0;
            Kind::Plain
        }
        21 | 22 => {
            c.grass = 25;
            c.flowers = 4;
            c.tree_density = 25.0;
            Kind::Jungle { big: false }
        }
        23 => {
            c.grass = 25;
            c.flowers = 4;
            c.tree_density = 2.0;
            Kind::Jungle { big: true }
        }
        35 | 36 => {
            c.tree_density = 1.0;
            c.flowers = 4;
            c.grass = 20;
            Kind::Savanna
        }
        37 | 38 | 39 | 165 | 166 | 167 => {
            c.tree_density = if id == 38 || id == 166 { 5.0 } else { -999.0 };
            c.flowers = 0;
            c.dead_bush = 20;
            c.reeds = 3;
            c.cactus = 5;
            Kind::Mesa
        }
        130 | 133 | 134 | 149 | 151 | 157 | 158 | 161 | 163 | 164 => Kind::Mutated(id - 128),
        155 | 156 => Kind::MutatedBirch(id - 128),
        _ => Kind::Plain, // 海洋、河流等：默认装饰器
    };
    (kind, c)
}

/// 装饰器的可变状态：只有 PlainsBiome::decorate 会改自己的计数器
/// （而 BiomeDecorator::decorate 又会读 `Biome::plains` 的 grassPerChunk）。
pub struct DecoState {
    /// biome 1、129 的 (flowerPerChunk, grassPerChunk)
    plains: (i32, i32),
    sunflower: (i32, i32),
    grass_noise: PerlinSimplexNoise,
}

impl Default for DecoState {
    fn default() -> Self {
        Self::new()
    }
}

impl DecoState {
    pub fn new() -> Self {
        let mut r = McpeRandom::new(2345);
        Self {
            plains: (4, 10),
            sunflower: (4, 10),
            grass_noise: PerlinSimplexNoise::new(&mut r, 1),
        }
    }

    fn counters(&self, id: u8) -> Counters {
        let (_, mut c) = biome_info(id);
        if id == 1 {
            (c.flowers, c.grass) = self.plains;
        } else if id == 129 {
            (c.flowers, c.grass) = self.sunflower;
        }
        c
    }
}

/// `biome->getTreeFeature(&random)`
fn tree_feature(kind: Kind, r: &mut McpeRandom) -> Tree {
    let default = |r: &mut McpeRandom| if r.next_int(10) == 0 { Tree::Fancy } else { Tree::Oak };
    match kind {
        Kind::Plains { .. } => {
            if r.next_int(3) == 0 {
                Tree::Fancy
            } else {
                Tree::Oak
            }
        }
        Kind::Forest(t) => {
            if t == 3 && r.next_int(3) == 0 {
                return Tree::Roof;
            }
            if t == 2 || r.next_int(5) == 0 {
                return Tree::Birch { tall: false };
            }
            default(r)
        }
        Kind::Taiga(t) => {
            if (t == 1 || t == 2) && r.next_int(3) == 0 {
                if t == 2 || r.next_int(13) == 0 {
                    return Tree::MegaPine { spruce: true };
                }
                return Tree::MegaPine { spruce: false };
            }
            if r.next_int(3) == 0 {
                Tree::Pine
            } else {
                Tree::Spruce
            }
        }
        Kind::ExtremeHills => {
            if r.next_int(3) == 0 {
                default(r)
            } else {
                Tree::Spruce
            }
        }
        Kind::Ice { .. } => Tree::Spruce,
        Kind::Jungle { big } => {
            if r.next_int(10) == 0 {
                return Tree::Fancy;
            }
            if r.next_int(2) == 0 {
                return Tree::GroundBush;
            }
            if !big && r.next_int(10) == 0 {
                return Tree::MegaJungle;
            }
            Tree::Jungle
        }
        Kind::Savanna => {
            if r.next_int(5) != 0 {
                Tree::Savanna
            } else {
                Tree::Oak
            }
        }
        Kind::Swamp => {
            if r.next_int(20) == 0 {
                Tree::HugeMushroom
            } else {
                Tree::Swamp
            }
        }
        Kind::Mesa => Tree::Oak,
        Kind::Mutated(base) => tree_feature(biome_info(base).0, r),
        Kind::MutatedBirch(_) => {
            if r.next_boolean() {
                Tree::Birch { tall: true }
            } else {
                Tree::Birch { tall: false }
            }
        }
        Kind::Plain | Kind::Desert | Kind::MushroomIsland => default(r),
    }
}

/// `biome->getGrassFeature(&random)` → tallgrass 的 data（1 草，2 蕨）
fn grass_feature(kind: Kind, r: &mut McpeRandom) -> u8 {
    match kind {
        Kind::Taiga(_) => {
            if r.next_int(5) != 0 {
                2
            } else {
                1
            }
        }
        Kind::Jungle { .. } => {
            if r.next_int(4) == 0 {
                2
            } else {
                1
            }
        }
        _ => 1,
    }
}

/// `biome->getRandomFlowerTypeAndData(random, pos)`
fn random_flower(st: &DecoState, kind: Kind, r: &mut McpeRandom, pos: (i32, i32, i32)) -> (u8, u8) {
    match kind {
        Kind::Plains { .. } => {
            let noise = st.grass_noise.get_value(pos.0 as f32 * 0.005, pos.2 as f32 * 0.005);
            if (noise as f64) < -0.8 {
                let t = r.next_int(4) as u8;
                (f::RED_FLOWER, t + 4)
            } else if r.next_int(3) > 0 {
                let t = r.next_int(3);
                (f::RED_FLOWER, [0u8, 3, 8][t as usize])
            } else {
                (f::YELLOW_FLOWER, 0)
            }
        }
        Kind::Forest(1) => {
            let noise = st
                .grass_noise
                .get_value(pos.0 as f32 * 0.02083333395421505, pos.2 as f32 * 0.02083333395421505);
            let mut v = 0.5f32 + noise * 0.5;
            if v > 0.9998999834060669 {
                v = 0.9998999834060669;
            } else if v > 0.0 {
            } else {
                v = 0.0;
            }
            let mut data = (v * 9.0) as i32 as u8;
            if data == 1 {
                data = 0;
            }
            (f::RED_FLOWER, data)
        }
        Kind::Swamp => (f::RED_FLOWER, 1),
        _ => (f::YELLOW_FLOWER, 0),
    }
}

#[inline]
fn random_height(s: &Src, origin: (i32, i32, i32), dx: i32, dz: i32, r: &mut McpeRandom) -> i32 {
    let h = s.heightmap(origin.0 + dx, origin.2 + dz) * 2;
    if h == 0 {
        0
    } else {
        r.next_int(h as u32) as i32
    }
}

/// `BiomeDecorator::decorateDepthSpan`
fn depth_span(
    s: &mut Src,
    r: &mut McpeRandom,
    o: (i32, i32, i32),
    count: i32,
    ore: (u8, u8, i32),
    min_y: i32,
    max_y: i32,
) {
    for _ in 0..count {
        let dx = r.next_int(16) as i32;
        let dy = r.next_int((max_y - min_y) as u32) as i32 + min_y;
        let dz = r.next_int(16) as i32;
        f::ore(s, (o.0 + dx, o.1 + dy, o.2 + dz), r, ore.0, ore.1, ore.2);
    }
}

/// `BiomeDecorator::decorateOres`
fn decorate_ores(s: &mut Src, r: &mut McpeRandom, o: (i32, i32, i32)) {
    depth_span(s, r, o, 10, (f::DIRT, 0, 33), 0, 128);
    depth_span(s, r, o, 8, (f::GRAVEL, 0, 33), 0, 128);
    if r.next_int(16) == 0 {
        depth_span(s, r, o, 80, (f::GRAVEL, 0, 33), 0, 50);
    }
    depth_span(s, r, o, 10, (f::STONE, 3, 33), 0, 80);
    depth_span(s, r, o, 10, (f::STONE, 1, 33), 0, 80);
    depth_span(s, r, o, 10, (f::STONE, 5, 33), 0, 80);
    depth_span(s, r, o, 20, (16, 0, 17), 0, 128);
    depth_span(s, r, o, 20, (15, 0, 9), 0, 64);
    depth_span(s, r, o, 2, (14, 0, 9), 0, 32);
    depth_span(s, r, o, 8, (73, 0, 8), 0, 16);
    depth_span(s, r, o, 1, (56, 0, 8), 0, 16);
    // decorateDepthAverage(lapis, 16, 16)
    let dx = r.next_int(16) as i32;
    let a = r.next_int(16) as i32;
    let b = r.next_int(16) as i32;
    let dy = a + b + 16 - 16;
    let dz = r.next_int(16) as i32;
    f::ore(s, (o.0 + dx, o.1 + dy, o.2 + dz), r, 21, 0, 7);
}

/// `BiomeDecorator::decorate(source, random, biome, origin, skipCaves, FLT_MAX)`
fn decorator_decorate(st: &DecoState, s: &mut Src, r: &mut McpeRandom, c: &Counters, kind: Kind, o: (i32, i32, i32)) {
    decorate_ores(s, r, o);
    for _ in 0..c.sand {
        let x = o.0 + r.next_int(16) as i32;
        let z = o.2 + r.next_int(16) as i32;
        let y = s.top_solid(x, z);
        f::sand_disk(s, (x, y, z), r, f::SAND, 7);
    }
    for _ in 0..c.clay {
        let x = o.0 + r.next_int(16) as i32;
        let z = o.2 + r.next_int(16) as i32;
        let y = s.top_solid(x, z);
        f::clay(s, (x, y, z), r, 4);
    }
    for _ in 0..c.gravel {
        let x = o.0 + r.next_int(16) as i32;
        let z = o.2 + r.next_int(16) as i32;
        let y = s.top_solid(x, z);
        f::sand_disk(s, (x, y, z), r, f::GRAVEL, 6);
    }
    let mut grass_count = c.grass;
    if c.tree_density > 0.0 {
        let trees: i32;
        if r.next_int(10) == 0 {
            grass_count = st.plains.1;
            trees = (r.next_float() * c.tree_density) as i32;
        } else if c.tree_density < 1.0 {
            trees = if r.next_float() < c.tree_density { 1 } else { 0 };
        } else {
            let mut t = c.tree_density as i32;
            let delta = r.next_int(10) as i32 - 7;
            if delta >= 0 {
                t += delta;
            }
            trees = t;
        }
        for _ in 0..trees {
            let tree = tree_feature(kind, r);
            let x = o.0 + 1 + r.next_int(14) as i32;
            let z = o.2 + 1 + r.next_int(14) as i32;
            let pos = (x, s.heightmap(x, z), z);
            f::place_tree(s, tree, pos, r);
            f::tall_grass(s, pos, r, 1, 23, 3);
        }
    }
    for _ in 0..c.huge_mushroom {
        let x = o.0 + r.next_int(16) as i32;
        let z = o.2 + r.next_int(16) as i32;
        let y = s.top_solid(x, z);
        f::huge_mushroom(s, (x, y, z), r);
    }
    for _ in 0..c.flowers {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let h = s.heightmap(o.0 + dx, o.2 + dz) + 32;
        let y = r.next_int(h as u32) as i32;
        let pos = (o.0 + dx, o.1 + y, o.2 + dz);
        let (id, data) = random_flower(st, kind, r, pos);
        // 原版判断的是该坐标的材质（不是花本身）：空气处不放
        if s.mat(pos.0, pos.1, pos.2) != 0 {
            f::place_flower(s, pos, id, data, r);
        }
    }
    if grass_count > 0 {
        for _ in 0..grass_count {
            let dx = r.next_int(16) as i32;
            let dz = r.next_int(16) as i32;
            let y = random_height(s, o, dx, dz, r);
            let data = grass_feature(kind, r);
            f::tall_grass(s, (o.0 + dx, o.1 + y, o.2 + dz), r, data, 90, 8);
        }
    }
    for _ in 0..c.dead_bush {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::dead_bush(s, (o.0 + dx, o.1 + y, o.2 + dz), r);
    }
    for _ in 0..c.waterlily {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let mut y = o.1 + random_height(s, o, dx, dz, r);
        let (x, z) = (o.0 + dx, o.2 + dz);
        while y > 0 {
            if !s.is_empty(x, y - 1, z) {
                break;
            }
            y -= 1;
        }
        f::waterlily(s, (x, y, z), r);
    }
    for _ in 0..c.mushrooms {
        if r.next_int(4) != 0 {
            if r.next_int(8) != 0 {
                continue;
            }
        } else {
            let dx = r.next_int(16) as i32;
            let dz = r.next_int(16) as i32;
            let (x, z) = (o.0 + dx, o.2 + dz);
            let y = s.heightmap(x, z);
            f::place_flower(s, (x, y, z), f::BROWN_MUSHROOM, 0, r);
            if r.next_int(8) != 0 {
                continue;
            }
        }
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::place_flower(s, (o.0 + dx, o.1 + y, o.2 + dz), f::RED_MUSHROOM, 0, r);
    }
    if r.next_int(4) == 0 {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::place_flower(s, (o.0 + dx, o.1 + y, o.2 + dz), f::BROWN_MUSHROOM, 0, r);
    }
    if r.next_int(8) == 0 {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::place_flower(s, (o.0 + dx, o.1 + y, o.2 + dz), f::RED_MUSHROOM, 0, r);
    }
    for _ in 0..c.reeds + 10 {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::reeds(s, (o.0 + dx, o.1 + y, o.2 + dz), r);
    }
    if r.next_int(32) == 0 {
        let x = o.0 + r.next_int(16) as i32;
        let z = o.2 + r.next_int(16) as i32;
        let y = r.next_int(s.heightmap(x, z) as u32) as i32;
        f::pumpkin(s, (x, y, z), r);
    }
    for _ in 0..c.cactus {
        let dx = r.next_int(16) as i32;
        let dz = r.next_int(16) as i32;
        let y = random_height(s, o, dx, dz, r);
        f::cactus(s, (o.0 + dx, o.1 + y, o.2 + dz), r);
    }
    // distance = FLT_MAX：泉水总是生成
    for _ in 0..50 {
        let x = o.0 + r.next_int(16) as i32;
        let inner = r.next_int(120) + 8;
        let y = r.next_int(inner) as i32;
        let z = o.2 + r.next_int(16) as i32;
        f::spring(s, (x, o.1 + y, z), r, f::FLOWING_WATER);
    }
    for _ in 0..20 {
        let x = o.0 + r.next_int(16) as i32;
        let a = r.next_int(112) + 8;
        let b = r.next_int(a) + 8;
        let y = r.next_int(b) as i32;
        let z = o.2 + r.next_int(16) as i32;
        f::spring(s, (x, o.1 + y, z), r, f::FLOWING_LAVA);
    }
    // FoliageColor::buildGrassColor：每列一次 nextInt(8)，之后不再用这个随机数，略
}

/// `biome->decorate(&source, random, chunkMin, skipCaves=false, FLT_MAX)`
pub fn decorate(st: &mut DecoState, s: &mut Src, r: &mut McpeRandom, biome: u8, o: (i32, i32, i32)) {
    let (kind, _) = biome_info(biome);
    match kind {
        Kind::Plains { mutated } => {
            let noise = st
                .grass_noise
                .get_value((o.0 + 8) as f32 * 0.005, (o.2 + 8) as f32 * 0.005) as f64;
            let slot = if biome == 1 { &mut st.plains } else { &mut st.sunflower };
            if noise < -0.8 {
                *slot = (15, 5);
            } else {
                *slot = (4, 10);
                for _ in 0..7 {
                    let dx = r.next_int(16) as i32;
                    let dz = r.next_int(16) as i32;
                    let (x, z) = (o.0 + dx, o.2 + dz);
                    let y = r.next_int((s.heightmap(x, z) + 32) as u32) as i32;
                    f::double_plant(s, (x, y, z), r, 2);
                }
            }
            if mutated {
                for _ in 0..10 {
                    let dx = r.next_int(16) as i32;
                    let dz = r.next_int(16) as i32;
                    let (x, z) = (o.0 + dx, o.2 + dz);
                    let y = r.next_int((s.heightmap(x, z) + 32) as u32) as i32;
                    f::double_plant(s, (x, y, z), r, 0);
                }
            }
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::Forest(t) => {
            if t == 3 {
                let mut i = 1;
                while i != 17 {
                    let mut j = 1;
                    while j != 17 {
                        let dx = i + r.next_int(3) as i32;
                        let dz = r.next_int(3) as i32 + j;
                        let (x, z) = (o.0 + dx, o.2 + dz);
                        let pos = (x, s.heightmap(x, z), z);
                        if r.next_int(20) == 0 {
                            f::huge_mushroom(s, pos, r);
                        } else {
                            let tree = tree_feature(kind, r);
                            f::place_tree(s, tree, pos, r);
                            f::tall_grass(s, pos, r, 1, 20, 3);
                        }
                        j += 4;
                    }
                    i += 4;
                }
            }
            let roll = r.next_int(5) as i32;
            let tries = if t != 1 { roll - 3 } else { roll - 1 };
            for _ in 0..tries.max(0) {
                let k = r.next_int(3);
                let data = if k == 0 {
                    1
                } else if k != 1 {
                    5
                } else {
                    4
                };
                let mut left = 5;
                loop {
                    let dx = r.next_int(16) as i32;
                    let dz = r.next_int(16) as i32;
                    let h = s.heightmap(o.0 + dx, o.2 + dz);
                    let y = r.next_int((h + 32) as u32) as i32;
                    if f::double_plant(s, (o.0 + dx, y, o.2 + dz), r, data) {
                        break;
                    }
                    left -= 1;
                    if left == 0 {
                        break;
                    }
                }
            }
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::Taiga(t) => {
            if t == 1 || t == 2 {
                let count = r.next_int(3);
                for _ in 0..count {
                    let dx = r.next_int(16) as i32;
                    let dz = r.next_int(16) as i32;
                    let (x, z) = (o.0 + dx, o.2 + dz);
                    let pos = (x, s.heightmap(x, z), z);
                    f::block_blob(s, pos, r, f::MOSSY_COBBLE, 0);
                }
            }
            let dx = r.next_int(16) as i32;
            let dz = r.next_int(16) as i32;
            let (x, z) = (o.0 + dx, o.2 + dz);
            let pos = (x, s.heightmap(x, z), z);
            f::double_plant(s, pos, r, 3);
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::ExtremeHills => {
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
            let count = 3 + r.next_int(6);
            for _ in 0..count {
                let rx = r.next_int(16) as i32;
                let ry = r.next_int(28) as i32 + 4;
                let rz = r.next_int(16) as i32;
                let (x, y, z) = (o.0 + rx, o.1 + ry, o.2 + rz);
                if s.get(x, y, z) == f::STONE {
                    s.set(x, y, z, f::EMERALD_ORE, 0);
                }
            }
            for _ in 0..7 {
                let rx = r.next_int(16) as i32;
                let ry = r.next_int(32) as i32;
                let rz = r.next_int(16) as i32;
                f::ore(s, (o.0 + rx, o.1 + ry, o.2 + rz), r, f::MONSTER_EGG, 0, 8);
            }
        }
        Kind::Desert => {
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
            if r.next_int(500) == 0 {
                let dx = r.next_int(16) as i32;
                let dz = r.next_int(16) as i32;
                f::desert_well(s, (o.0 + dx, o.1 + 128, o.2 + dz));
            }
        }
        Kind::Ice { spikes } => {
            if spikes {
                for _ in 0..3 {
                    let x = o.0 + r.next_int(16) as i32;
                    let z = o.2 + r.next_int(16) as i32;
                    let pos = (x, s.heightmap(x, z), z);
                    f::ice_spike(s, pos, r);
                }
                for _ in 0..2 {
                    let x = o.0 + r.next_int(16) as i32;
                    let z = o.2 + r.next_int(16) as i32;
                    let pos = (x, s.heightmap(x, z), z);
                    f::ice_patch(s, pos, r, 4);
                }
            }
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::Jungle { .. } => {
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
            let dx = r.next_int(16) as i32;
            let dz = r.next_int(16) as i32;
            let (x, z) = (o.0 + dx, o.2 + dz);
            let h = s.heightmap(x, z) * 2;
            let y = if h == 0 { 0 } else { r.next_int(h as u32) as i32 };
            f::melon(s, (x, y, z), r);
            for _ in 0..50 {
                let rx = r.next_int(16) as i32 + 8;
                let rz = r.next_int(16) as i32 + 8;
                f::vines(s, (o.0 + rx, o.1 + 64, o.2 + rz), r);
            }
        }
        Kind::Savanna => {
            let dx = r.next_int(16) as i32;
            let dz = r.next_int(16) as i32;
            let (x, z) = (o.0 + dx, o.2 + dz);
            let pos = (x, s.heightmap(x, z), z);
            f::double_plant(s, pos, r, 2);
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::Mutated(base) | Kind::MutatedBirch(base) => {
            // MutatedBiome::decorate：基础 biome 的装饰器，biome 参数是自己
            let c = st.counters(base);
            decorator_decorate(st, s, r, &c, kind, o);
        }
        Kind::Swamp | Kind::MushroomIsland | Kind::Mesa | Kind::Plain => {
            let c = st.counters(biome);
            decorator_decorate(st, s, r, &c, kind, o);
        }
    }
}
