// 对齐 PopulatorOre 与 ObjectOre：沿一条随机方向的锥形长条替换石头
use std::f64::consts::PI;

use crate::level::generator::blocks::{
    COAL_ORE, DIAMOND_ORE, DIRT, GOLD_ORE, GRAVEL, IRON_ORE, LAPIS_ORE, REDSTONE_ORE, STONE,
};
use crate::level::generator::populator::random_range;
use crate::level::generator::random::NukkitRandom;
use crate::level::generator::world::GenWorld;

pub struct OreType {
    pub material: u8,
    pub cluster_count: i32,
    pub cluster_size: i32,
    pub min_height: i32,
    pub max_height: i32,
}

// Normal 生成器注册的矿脉配置 顺序与 Nukkit 一致
pub const ORE_TYPES: [OreType; 8] = [
    OreType { material: COAL_ORE, cluster_count: 20, cluster_size: 16, min_height: 0, max_height: 128 },
    OreType { material: IRON_ORE, cluster_count: 20, cluster_size: 8, min_height: 0, max_height: 64 },
    OreType { material: REDSTONE_ORE, cluster_count: 8, cluster_size: 7, min_height: 0, max_height: 16 },
    OreType { material: LAPIS_ORE, cluster_count: 1, cluster_size: 6, min_height: 0, max_height: 32 },
    OreType { material: GOLD_ORE, cluster_count: 2, cluster_size: 8, min_height: 0, max_height: 32 },
    OreType { material: DIAMOND_ORE, cluster_count: 1, cluster_size: 7, min_height: 0, max_height: 16 },
    OreType { material: DIRT, cluster_count: 20, cluster_size: 32, min_height: 0, max_height: 128 },
    OreType { material: GRAVEL, cluster_count: 10, cluster_size: 16, min_height: 0, max_height: 128 },
];

pub fn populate(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, random: &mut NukkitRandom) {
    for ore_type in ORE_TYPES.iter() {
        for _ in 0..ore_type.cluster_count {
            let x = random_range(random, chunk_x << 4, (chunk_x << 4) + 15);
            let y = random_range(random, ore_type.min_height, ore_type.max_height);
            let z = random_range(random, chunk_z << 4, (chunk_z << 4) + 15);
            // 起点必须是石头 否则这个矿脉整条跳过
            if world.block(x, y, z) == STONE {
                place_object(world, random, ore_type, x, y, z);
            }
        }
    }
}

// 对齐 ObjectOre.placeObject：从一条线段上撒若干球体
fn place_object(
    world: &mut GenWorld,
    random: &mut NukkitRandom,
    ore_type: &OreType,
    x: i32,
    y: i32,
    z: i32,
) {
    let cluster_size = ore_type.cluster_size;
    let angle = random.next_float() as f64 * PI;
    let offset_x = angle.cos() * cluster_size as f64 / 8.0;
    let offset_y = angle.sin() * cluster_size as f64 / 8.0;

    let x1 = x as f64 + 8.0 + offset_x;
    let x2 = x as f64 + 8.0 - offset_x;
    let z1 = z as f64 + 8.0 + offset_y;
    let z2 = z as f64 + 8.0 - offset_y;
    let y1 = y as f64 + random.next_bounded_int(3) as f64 + 2.0;
    let y2 = y as f64 + random.next_bounded_int(3) as f64 + 2.0;

    for count in 0..=cluster_size {
        let ratio = count as f64 / cluster_size as f64;
        let seed_x = x1 + (x2 - x1) * ratio;
        let seed_y = y1 + (y2 - y1) * ratio;
        let seed_z = z1 + (z2 - z1) * ratio;
        let size = ((count as f64 * (PI / cluster_size as f64)).sin() + 1.0)
            * random.next_float() as f64
            * cluster_size as f64
            / 16.0
            + 1.0;
        let size = size / 2.0;

        let start_x = (seed_x - size) as i32;
        let start_y = (seed_y - size) as i32;
        let start_z = (seed_z - size) as i32;
        let end_x = (seed_x + size) as i32;
        let end_y = (seed_y + size) as i32;
        let end_z = (seed_z + size) as i32;

        for bx in start_x..=end_x {
            let size_x = (bx as f64 + 0.5 - seed_x) / size;
            let size_x = size_x * size_x;
            if size_x >= 1.0 {
                continue;
            }
            for by in start_y..=end_y {
                let size_y = (by as f64 + 0.5 - seed_y) / size;
                let size_y = size_y * size_y;
                if by <= 0 || size_x + size_y >= 1.0 {
                    continue;
                }
                for bz in start_z..=end_z {
                    let size_z = (bz as f64 + 0.5 - seed_z) / size;
                    let size_z = size_z * size_z;
                    if size_x + size_y + size_z < 1.0 && world.block(bx, by, bz) == STONE {
                        world.set_block(bx, by, bz, ore_type.material);
                    }
                }
            }
        }
    }
}
