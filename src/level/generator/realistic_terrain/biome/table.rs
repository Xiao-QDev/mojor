//! 对应 `rtg/biome/Biome.php`。
//!
//! PHP 里用数组 + 字符串类名（`Biome::getBiomeClass() == 'BiomeJungle'`）判断，
//! 这里换成枚举，比较更快，也避免把类型名拼错。
//!
//! 注意 `getIdForBiome($x)` 在 PHP 里是**恒等映射**（`null → -1`），
//! 所以 Rust 侧直接保留原始 id，只在需要判空的地方用 `Option<i32>`。

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BiomeClass {
    Ocean,
    Plains,
    Desert,
    Hills,
    Forest,
    Taiga,
    Swamp,
    River,
    Hell,
    End,
    Snow,
    MushroomIsland,
    Beach,
    Jungle,
    StoneBeach,
    Savanna,
    Mesa,
    Void,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TempCategory {
    Ocean,
    Cold,
    Medium,
    Warm,
}

/// `(类名, 温度, 是否积雪)`
fn info(id: i32) -> Option<(BiomeClass, f32, bool)> {
    use BiomeClass::*;
    Some(match id {
        0 => (Ocean, 0.5, false),
        1 => (Plains, 0.8, false),
        2 => (Desert, 2.0, false),
        3 => (Hills, 0.2, false),
        4 => (Forest, 0.7, false),
        5 => (Taiga, 0.25, false),
        6 => (Swamp, 0.8, false),
        7 => (River, 0.5, false),
        8 => (Hell, 2.0, false),
        9 => (End, 0.5, false),
        10 => (Ocean, 0.0, true),
        11 => (River, 0.0, true),
        12 => (Snow, 0.0, true),
        13 => (Snow, 0.0, true),
        14 => (MushroomIsland, 0.9, false),
        15 => (MushroomIsland, 0.9, false),
        16 => (Beach, 0.8, false),
        17 => (Desert, 2.0, false),
        18 => (Forest, 0.7, false),
        19 => (Taiga, 0.25, false),
        20 => (Hills, 0.2, false),
        21 => (Jungle, 0.95, false),
        22 => (Jungle, 0.95, false),
        23 => (Jungle, 0.95, false),
        24 => (Ocean, 0.5, false),
        25 => (StoneBeach, 0.2, false),
        26 => (Beach, 0.05, true),
        27 => (Forest, 0.6, false),
        28 => (Forest, 0.6, false),
        29 => (Forest, 0.7, false),
        30 => (Taiga, -0.5, true),
        31 => (Taiga, -0.5, true),
        32 => (Taiga, 0.3, false),
        33 => (Taiga, 0.3, false),
        34 => (Hills, 0.2, false),
        35 => (Savanna, 1.2, false),
        36 => (Savanna, 1.0, false),
        37 => (Mesa, 2.0, false),
        38 => (Mesa, 2.0, false),
        39 => (Mesa, 2.0, false),
        127 => (Void, 0.5, false),
        129 => (Plains, 0.8, false),
        130 => (Desert, 2.0, false),
        131 => (Hills, 0.2, false),
        132 => (Forest, 0.7, false),
        133 => (Taiga, 0.25, false),
        134 => (Swamp, 0.8, false),
        140 => (Snow, 0.0, true),
        149 => (Jungle, 0.95, false),
        151 => (Jungle, 0.95, false),
        155 => (Forest, 0.6, false),
        156 => (Forest, 0.6, false),
        157 => (Forest, 0.7, false),
        158 => (Taiga, -0.5, true),
        160 => (Taiga, 0.25, false),
        161 => (Taiga, 0.25, false),
        162 => (Hills, 0.2, false),
        163 => (Savanna, 1.1, false),
        164 => (Savanna, 1.0, false),
        165 => (Mesa, 2.0, false),
        166 => (Mesa, 2.0, false),
        167 => (Mesa, 2.0, false),
        _ => return None,
    })
}

/// `Biome::getBiome($id)`
pub fn get_biome(id: i32) -> Option<i32> {
    info(id).map(|_| id)
}

pub fn biome_class(id: i32) -> Option<BiomeClass> {
    info(id).map(|(class, _, _)| class)
}

pub fn is_snowy(id: i32) -> bool {
    info(id).map_or(false, |(_, _, snowy)| snowy)
}

pub fn temp_category(id: i32) -> Option<TempCategory> {
    let (class, temp, _) = info(id)?;
    Some(if class == BiomeClass::Ocean {
        TempCategory::Ocean
    } else if temp < 0.2 {
        TempCategory::Cold
    } else if temp < 1.0 {
        TempCategory::Medium
    } else {
        TempCategory::Warm
    })
}

pub fn is_mutation(id: i32) -> bool {
    id >= 128
}

/// `Biome::getMutationForBiome($id)` = `getBiome($id + 128)`。
/// PHP 里传 null 时 `null + 128 == 128`，同样查不到，结果一致。
pub fn mutation_for(id: Option<i32>) -> Option<i32> {
    id.and_then(|v| get_biome(v + 128))
}

/// `Biome::getIdForBiome($x)`：恒等映射，`null` 变 -1。
pub fn id_of(id: Option<i32>) -> i32 {
    id.unwrap_or(-1)
}

/// `GenLayer::biomesEqualOrMesaPlateau`
pub fn biomes_equal_or_mesa_plateau(a: i32, b: i32) -> bool {
    if a == b {
        return true;
    }
    match (get_biome(a), get_biome(b)) {
        (Some(biome), Some(biome1)) => {
            if biome != 38 && biome != 39 {
                biome == biome1 || biome_class(biome) == biome_class(biome1)
            } else {
                biome1 == 38 || biome1 == 39
            }
        }
        _ => false,
    }
}
