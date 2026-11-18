//! 装饰阶段 = `RandomLevelSource::postProcess`，按 MCPE 0.14.1 重建源码移植
//! （`src/common/world/level/levelgen/RandomLevelSource.cpp`、`feature/LakeFeature.cpp`）。
//!
//! 已移植：湖 / 岩浆湖、结冰。
//! 未移植：村庄、废弃矿井、要塞、神殿（结构内部会重新播种，不影响后面 decorate 的随机数）、
//! 地牢、biome 装饰器（矿石、树、花草……）、_fixWaterAlongEdges、液体 tick。

use crate::level::generator::blocks::{AIR, DIRT, GRASS, ICE, STILL_LAVA, STILL_WATER, STONE, WATER};
use crate::level::generator::mcpe_noise::McpeRandom;
use crate::level::generator::world::GenWorld;

const LAVA: u8 = 10;
const DESERT: u8 = 2;
const DESERT_HILLS: u8 = 17;

/// 液体（Material::isLiquid）
fn is_liquid(id: u8) -> bool {
    matches!(id, WATER | STILL_WATER | LAVA | STILL_LAVA)
}

/// 不挡路、也不是固体的方块（空气、植物类、雪层、火……）
fn is_non_solid(id: u8) -> bool {
    matches!(
        id,
        AIR | 6 | 31 | 32 | 37 | 38 | 39 | 40 | 50 | 51 | 55 | 59 | 78 | 83 | 104 | 105 | 106 | 111 | 141 | 142 | 175 | 244
    )
}

/// `Material::isSolid`
fn is_solid(id: u8) -> bool {
    !is_liquid(id) && !is_non_solid(id)
}

/// `Material::getBlocksMotion`
fn blocks_motion(id: u8) -> bool {
    is_solid(id)
}

/// `Block::mLightBlock`（只区分装饰阶段会遇到的方块）
fn light_block(id: u8) -> i32 {
    match id {
        AIR | 6 | 20 | 31 | 32 | 37 | 38 | 39 | 40 | 50 | 51 | 55 | 59 | 78 | 83 | 102 | 104 | 105 | 106 | 111
        | 141 | 142 | 175 | 244 => 0,
        18 | 161 | 30 => 1,
        WATER | STILL_WATER | ICE => 3,
        _ => 15,
    }
}

/// 天空光是否 > 0（按当前方块从顶向下累减，近似 recalcHeightmap 算出的 skylight）
fn sky_open(world: &GenWorld, x: i32, y: i32, z: i32) -> bool {
    let mut light = 15;
    let mut yy = 127;
    while yy >= y {
        light -= light_block(world.block(x, yy, z));
        if light <= 0 {
            return false;
        }
        yy -= 1;
    }
    true
}

/// biome 温度（`Biome::getTemperature`，不含高度修正）
fn temperature(id: u8) -> f32 {
    match id {
        10 | 11 | 12 | 13 | 140 => 0.0,
        26 => 0.05,
        30 | 31 | 158 => -0.5,
        _ => 1.0, // 结冰只关心是否 ≤ 0.15
    }
}

/// `RandomLevelSource::postProcess` 中已移植的部分
pub fn post_process(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, seed: u32, skip_caves: bool) {
    let base_x = chunk_x << 4;
    let base_z = chunk_z << 4;
    // source.getBiome(chunk->getMax())：区块的最大角
    let biome = world.biome(base_x + 15, base_z + 15);

    let mut rng = McpeRandom::new(seed);
    let xs = ((rng.genrand() >> 1) as i32 / 2) * 2 + 1;
    let zs = ((rng.genrand() >> 1) as i32 / 2) * 2 + 1;
    let lake_seed = chunk_x.wrapping_mul(xs).wrapping_add(chunk_z.wrapping_mul(zs));
    let mut rng = McpeRandom::new(seed ^ lake_seed as u32);

    let desert = biome == DESERT || biome == DESERT_HILLS;
    if desert || rng.next_int(4) != 0 {
        if !skip_caves && rng.next_int(8) == 0 {
            let dx = rng.next_int(9) as i32 + 3;
            let inner = rng.next_int(120) + 8;
            let y = rng.next_int(inner) as i32;
            let dz = rng.next_int(9) as i32 + 3;
            if y > 59 && (y <= 63 || rng.next_int(10) == 0) {
                place_lake(world, base_x + dx, y, base_z + dz, &mut rng, STILL_LAVA, AIR);
            }
        }
    } else {
        let dx = rng.next_int(9) as i32 + 3;
        let y = rng.next_int(128) as i32;
        let dz = rng.next_int(9) as i32 + 3;
        if !(y <= 59 && skip_caves) {
            place_lake(world, base_x + dx, y, base_z + dz, &mut rng, STILL_WATER, AIR);
        }
    }

    // 结冰：每列最高的"挡雨"方块下面一格若是静止水（数据 0）且 biome 温度 ≤ 0.15 → 冰
    for dx in 0..16 {
        for dz in 0..16 {
            let (x, z) = (base_x + dx, base_z + dz);
            let top = top_rain_y(world, x, z);
            let y = top - 1;
            if !(0..128).contains(&y) {
                continue;
            }
            if temperature(world.biome(x, z)) > 0.15 {
                continue;
            }
            let id = world.block(x, y, z);
            if (id == STILL_WATER || id == WATER) && world.meta(x, y, z) == 0 {
                world.set_block_with_meta(x, y, z, ICE, 0);
            }
        }
    }
}

/// `LevelChunk::getTopRainBlockPos` 的 y
fn top_rain_y(world: &GenWorld, x: i32, z: i32) -> i32 {
    let mut y = 127;
    while y > 0 {
        let id = world.block(x, y, z);
        if blocks_motion(id) || is_liquid(id) {
            return (y + 1).min(127);
        }
        y -= 1;
    }
    -1
}

/// `LakeFeature::place`
fn place_lake(world: &mut GenWorld, px: i32, py: i32, pz: i32, rng: &mut McpeRandom, block: u8, edge: u8) -> bool {
    let ox = px - 8;
    let oz = pz - 8;
    let mut oy = py;
    while oy > 0 && world.block(ox, oy, oz) == AIR {
        oy -= 1;
    }
    oy -= 4;

    let mut lake = [false; 16 * 16 * 8];
    let idx = |x: i32, y: i32, z: i32| ((x * 16 + z) * 8 + y) as usize;
    let n = rng.next_int(4) + 4;
    for _ in 0..n {
        let sx = rng.next_float() * 6.0 + 3.0;
        let sy = rng.next_float() * 4.0 + 2.0;
        let sz = rng.next_float() * 6.0 + 3.0;
        let hx = sx / 2.0;
        let hy = sy / 2.0;
        let hz = sz / 2.0;
        let cx = rng.next_float() * (16.0 - sx - 2.0 + 1.0 + hx);
        let cy = rng.next_float() * (8.0 - sy - 4.0 + 2.0 + hy);
        let cz = rng.next_float() * (16.0 - sz - 2.0 + 1.0 + hz);
        for x in 1..15 {
            for z in 1..15 {
                for y in 1..7 {
                    let dx = (x as f32 - cx) / hx;
                    let dy = (y as f32 - cy) / hy;
                    let dz = (z as f32 - cz) / hz;
                    if dx * dx + dy * dy + dz * dz < 1.0 {
                        lake[idx(x, y, z)] = true;
                    }
                }
            }
        }
    }

    let check = |lake: &[bool], x: i32, y: i32, z: i32| -> bool {
        !lake[idx(x, y, z)]
            && ((x < 15 && lake[idx(x + 1, y, z)])
                || (x > 0 && lake[idx(x - 1, y, z)])
                || (z < 15 && lake[idx(x, y, z + 1)])
                || (z > 0 && lake[idx(x, y, z - 1)])
                || (y < 7 && lake[idx(x, y + 1, z)])
                || (y > 0 && lake[idx(x, y - 1, z)]))
    };

    for x in 0..16 {
        for z in 0..16 {
            for y in 0..8 {
                if check(&lake, x, y, z) {
                    let id = world.block(ox + x, oy + y, oz + z);
                    if y >= 4 && is_liquid(id) {
                        return false;
                    }
                    if y < 4 && !is_solid(id) && id != block {
                        return false;
                    }
                }
            }
        }
    }

    // 泥土换草要用放湖之前的天空光（setBlockNoUpdate 不更新光照）
    let mut sky = [false; 16 * 16 * 8];
    for x in 0..16 {
        for z in 0..16 {
            for y in 4..8 {
                if lake[idx(x, y, z)] {
                    sky[idx(x, y, z)] = sky_open(world, ox + x, oy + y, oz + z);
                }
            }
        }
    }

    for x in 0..16 {
        for z in 0..16 {
            for y in 0..8 {
                if lake[idx(x, y, z)] {
                    world.set_block_with_meta(ox + x, oy + y, oz + z, if y >= 4 { edge } else { block }, 0);
                }
            }
        }
    }

    for x in 0..16 {
        for z in 0..16 {
            for y in 4..8 {
                if lake[idx(x, y, z)]
                    && world.block(ox + x, oy + y - 1, oz + z) == DIRT
                    && sky[idx(x, y, z)]
                {
                    world.set_block_with_meta(ox + x, oy + y - 1, oz + z, GRASS, 0);
                }
            }
        }
    }

    if block == STILL_LAVA || block == LAVA {
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..8 {
                    if check(&lake, x, y, z)
                        && (y < 4 || rng.next_int(2) != 0)
                        && is_solid(world.block(ox + x, oy + y, oz + z))
                    {
                        world.set_block_with_meta(ox + x, oy + y, oz + z, STONE, 0);
                    }
                }
            }
        }
    }
    true
}
