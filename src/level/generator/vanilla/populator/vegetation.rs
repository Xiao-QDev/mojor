// 对齐 PopulatorGrass / TallGrass / Flower / Sugarcane / Cactus / DeadBush / LilyPad
use crate::level::generator::blocks::*;
use crate::level::generator::populator::{highest_workable_block, random_range};
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

// 长在草方块上的植被共用这套顶部扫描规则
const PLANT_IGNORE: [u8; 4] = [AIR, LEAVES, LEAVES2, SNOW_LAYER];
// 甘蔗只忽略叶子和空气
const CANE_IGNORE: [u8; 3] = [AIR, LEAVES, LEAVES2];

pub fn grass(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        // 上面是空气或雪层 下面是草
        let above = world.block(x, y, z);
        if (above == AIR || above == SNOW_LAYER) && world.block(x, y - 1, z) == GRASS {
            world.set_block(x, y, z, TALL_GRASS);
            world.set_meta(x, y, z, 0);
        }
    }
}

pub fn tall_grass(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        let above = world.block(x, y, z);
        if (above == AIR || above == SNOW_LAYER)
            && world.block(x, y - 1, z) == GRASS
            && world.block(x, y + 1, z) == AIR
        {
            world.set_block(x, y, z, DOUBLE_PLANT);
            world.set_meta(x, y, z, 2);
            world.set_block(x, y + 1, z, DOUBLE_PLANT);
            world.set_meta(x, y + 1, z, 10);
        }
    }
}

pub fn flower(
    world: &mut GenWorld,
    chunk_x: i32,
    chunk_z: i32,
    random: &mut NukkitRandom,
    base: i32,
    random_amount: i32,
    types: &[(u8, u8)],
) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    if types.is_empty() {
        return;
    }
    let end = types.len() as i32;

    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        let above = world.block(x, y, z);
        if (above == AIR || above == SNOW_LAYER) && world.block(x, y - 1, z) == GRASS {
            let pick = random.next_range(0, end - 1) as usize;
            let (id, meta) = types[pick];
            world.set_block(x, y, z, id);
            world.set_meta(x, y, z, meta);
        }
    }
}

pub fn sugarcane(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &CANE_IGNORE);
        if y == -1 {
            continue;
        }
        let below = world.block(x, y - 1, z);
        if world.block(x, y, z) == AIR
            && (below == SAND || below == GRASS)
            && find_water(world, x, y - 1, z)
        {
            world.set_block(x, y, z, SUGARCANE);
            world.set_meta(x, y, z, 1);
        }
    }
}

// 对齐 PopulatorTallSugarcane：踩在甘蔗头上再长一节
pub fn tall_sugarcane(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &CANE_IGNORE);
        if y == -1 {
            continue;
        }
        if world.block(x, y, z) == AIR && world.meta(x, y - 1, z) == SUGARCANE {
            world.set_block(x, y, z, SUGARCANE);
            world.set_meta(x, y, z, 1);
        }
    }
}

pub fn cactus(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        if world.block(x, y, z) == AIR
            && world.block(x, y - 1, z) == SAND
            && world.block(x + 1, y, z) == AIR
            && world.block(x - 1, y, z) == AIR
            && world.block(x, y, z + 1) == AIR
            && world.block(x, y, z - 1) == AIR
        {
            world.set_block(x, y, z, CACTUS);
            world.set_meta(x, y, z, 1);
        }
    }
}

pub fn dead_bush(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        if world.block(x, y, z) == AIR && world.block(x, y - 1, z) == SAND {
            world.set_block(x, y, z, DEAD_BUSH);
            world.set_meta(x, y, z, 1);
        }
    }
}

pub fn lily_pad(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom, base: i32, random_amount: i32) {
    let amount = random.next_bounded_int(random_amount + 1) + base;
    for _ in 0..amount {
        let (x, z, y) = next_spot(world, chunk_x, chunk_z, random, &PLANT_IGNORE);
        if y == -1 {
            continue;
        }
        let above = world.block(x, y, z);
        if (above == AIR || above == SNOW_LAYER) && world.block(x, y - 1, z) == STILL_WATER {
            world.set_block(x, y, z, WATER_LILY);
            world.set_meta(x, y, z, 1);
        }
    }
}

// 先摇两个坐标再找落点 随机数顺序与 Nukkit 一致
fn next_spot(
    world: &GenWorld,
    chunk_x: i32,
    chunk_z: i32,
    random: &mut NukkitRandom,
    ignore: &[u8],
) -> (i32, i32, i32) {
    let x = random_range(random, chunk_x * 16, chunk_x * 16 + 15);
    let z = random_range(random, chunk_z * 16, chunk_z * 16 + 15);
    let y = highest_workable_block(world, x, z, ignore);
    (x, z, y)
}

// 周围 8x8 范围内水面超过 10 格才允许种甘蔗
fn find_water(world: &GenWorld, x: i32, y: i32, z: i32) -> bool {
    let mut count = 0;
    for i in (x - 4)..(x + 4) {
        for j in (z - 4)..(z + 4) {
            let id = world.block(i, y, j);
            if id == WATER || id == STILL_WATER {
                count += 1;
            }
            if count > 10 {
                return true;
            }
        }
    }
    count > 10
}
