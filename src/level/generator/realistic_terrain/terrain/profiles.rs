//! 对应 `rtg/terrain/BiomeProfiles.php`。
//!
//! 每个 biome 的六元组：`[允许河流, 允许湖泊, 河流 biome, 沙滩 biome, 渗入, 渗出]`。
//! 目前 `LandscapeGenerator` 只用到前两项，后四项留给 surface 层。

#[derive(Clone, Copy, Debug)]
pub struct BiomeProfile {
    pub allow_rivers: bool,
    pub allow_lakes: bool,
    pub river_biome: i32,
    pub beach_biome: i32,
    pub bleed_in: bool,
    pub bleed_out: bool,
}

const fn p(
    allow_rivers: bool,
    allow_lakes: bool,
    river_biome: i32,
    beach_biome: i32,
    bleed_in: bool,
    bleed_out: bool,
) -> BiomeProfile {
    BiomeProfile {
        allow_rivers,
        allow_lakes,
        river_biome,
        beach_biome,
        bleed_in,
        bleed_out,
    }
}

/// 5 个台地/峡谷 biome 会被当成一个整体：混合时共享同一个 `border`。
pub fn is_mesa(id: i32) -> bool {
    matches!(id, 38 | 39 | 165 | 166 | 167)
}

pub fn get(id: i32) -> Option<BiomeProfile> {
    Some(match id {
        16 => p(true, true, 7, 16, true, true),
        27 => p(true, true, 7, 16, false, false),
        28 => p(true, false, 7, 16, false, false),
        156 => p(true, false, 7, 16, false, false),
        155 => p(true, false, 7, 16, false, false),
        26 => p(true, true, 11, 26, false, false),
        30 => p(true, true, 11, 26, false, false),
        31 => p(true, false, 11, 26, false, false),
        158 => p(false, false, 11, 26, false, false),
        24 => p(false, false, 7, 16, false, false),
        2 => p(true, false, 7, 16, false, false),
        17 => p(true, false, 7, 16, false, false),
        130 => p(false, false, 7, 16, false, false),
        3 => p(false, false, 7, 25, false, false),
        20 => p(false, false, 7, 25, false, false),
        131 => p(false, true, 7, 25, false, false),
        34 => p(false, false, 7, 25, false, false),
        162 => p(false, false, 7, 25, false, false),
        132 => p(true, true, 7, 16, false, false),
        4 => p(true, true, 7, 16, false, false),
        18 => p(true, false, 7, 16, false, false),
        10 => p(true, false, 11, 26, false, false),
        11 => p(true, true, 11, 26, false, false),
        13 => p(false, false, 11, 26, false, false),
        12 => p(true, true, 11, 26, false, false),
        140 => p(true, false, 11, 26, false, false),
        21 => p(true, true, 7, 16, false, false),
        23 => p(true, true, 7, 16, false, false),
        151 => p(false, false, 7, 16, false, false),
        22 => p(true, false, 7, 16, false, false),
        149 => p(false, false, 7, 16, false, false),
        160 => p(true, false, 7, 25, false, false),
        32 => p(true, true, 7, 25, false, false),
        33 => p(true, false, 7, 25, false, false),
        37 => p(true, true, 7, 37, false, false),
        165 => p(true, true, 7, 165, false, false),
        39 => p(true, false, 7, 39, false, false),
        38 => p(true, false, 7, 38, false, false),
        166 => p(true, false, 7, 166, false, false),
        167 => p(true, false, 7, 167, false, false),
        14 => p(true, false, 7, 15, false, false),
        15 => p(true, false, 7, 15, false, false),
        0 => p(false, false, 7, 16, false, false),
        1 => p(true, true, 7, 16, false, false),
        161 => p(true, false, 7, 25, false, false),
        7 => p(true, true, 7, 16, false, false),
        29 => p(true, true, 7, 16, false, false),
        157 => p(true, false, 7, 16, false, false),
        35 => p(true, true, 7, 16, false, false),
        163 => p(true, false, 7, 16, false, false),
        36 => p(true, false, 7, 16, false, false),
        164 => p(true, false, 7, 16, false, false),
        25 => p(true, true, 7, 25, true, true),
        129 => p(true, true, 7, 16, false, false),
        6 => p(true, true, 7, 6, false, false),
        134 => p(true, true, 7, 134, false, false),
        5 => p(true, true, 7, 25, false, false),
        19 => p(true, false, 7, 25, false, false),
        133 => p(true, false, 7, 25, false, false),
        _ => return None,
    })
}
