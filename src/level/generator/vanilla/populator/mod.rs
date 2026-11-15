// 对齐 Nukkit 的 populator 包：地表覆盖 洞穴 矿石 树木 植被
pub mod caves;
pub mod ground_cover;
pub mod ore;
pub mod tree;
pub mod vegetation;

use crate::level::format::chunk::CHUNK_HEIGHT;
use crate::level::generator::biome::PopulatorSpec;
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

// 按群系配置分派到具体装饰器
pub fn run_spec(
    spec: &PopulatorSpec,
    world: &mut GenWorld,
    chunk_x: i32,
    chunk_z: i32,
    random: &mut NukkitRandom,
) {
    match *spec {
        PopulatorSpec::Tree {
            sapling,
            base,
            random: extra,
        } => tree::populate(world, chunk_x, chunk_z, random, sapling, base, extra),
        PopulatorSpec::Grass {
            base,
            random: extra,
        } => vegetation::grass(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::TallGrass {
            base,
            random: extra,
        } => vegetation::tall_grass(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::Flower {
            base,
            random: extra,
            types,
        } => vegetation::flower(world, chunk_x, chunk_z, random, base, extra, types),
        PopulatorSpec::Sugarcane {
            base,
            random: extra,
        } => vegetation::sugarcane(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::TallSugarcane {
            base,
            random: extra,
        } => vegetation::tall_sugarcane(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::Cactus {
            base,
            random: extra,
        } => vegetation::cactus(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::DeadBush {
            base,
            random: extra,
        } => vegetation::dead_bush(world, chunk_x, chunk_z, random, base, extra),
        PopulatorSpec::LilyPad {
            base,
            random: extra,
        } => vegetation::lily_pad(world, chunk_x, chunk_z, random, base, extra),
    }
}

// 对齐 NukkitMath.randomRange 闭区间
pub fn random_range(random: &mut NukkitRandom, start: i32, end: i32) -> i32 {
    start + (random.next_int() % (end + 1 - start))
}

// 从顶部往下找第一个不属于忽略表的方块 返回它上面一格
// 找不到可落脚的层（只到 y=0）时返回 -1
pub fn highest_workable_block(world: &GenWorld, x: i32, z: i32, ignore: &[u8]) -> i32 {
    let mut y = CHUNK_HEIGHT as i32 - 1;
    while y >= 0 {
        let id = world.block(x, y, z);
        if !ignore.contains(&id) {
            break;
        }
        y -= 1;
    }
    if y <= 0 { -1 } else { y + 1 }
}
