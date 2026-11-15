// 对齐 PopulatorCaves：3D 噪声挖洞 20 层以下灌岩浆
use crate::level::generator::biome::{OCEAN, RIVER};
use crate::level::generator::blocks::{AIR, DIRT, GRASS, LAVA, STILL_WATER, WATER};
use crate::level::generator::noise::{Noise, fast_noise3d};
use crate::level::generator::world::GenWorld;

pub fn populate(world: &mut GenWorld, chunk_x: i32, chunk_z: i32, caves: &Noise) {
    let base_x = chunk_x * 16;
    let base_z = chunk_z * 16;

    let array = fast_noise3d(caves, 16, 128, 16, 4, 4, 4, base_x, 0, base_z);
    let index = |x: i32, z: i32, y: i32| ((x * 17 + z) * 129 + y) as usize;

    // 洞穴只在本区块内打洞 直接借区块 避免逐格 HashMap 查找
    let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
        return;
    };

    for x in 0..16usize {
        for z in 0..16usize {
            let biome_id = chunk.biome_id(x, z);
            // WateryBiome 的区块一开始就当作有水 湖底不再长草
            let mut has_water = biome_id == OCEAN || biome_id == RIVER;
            let mut highest_block = !has_water;

            let mut y = 127i32;
            while y >= 20 {
                let block = chunk.block(x, y as usize, z);

                if block == AIR {
                    y -= 1;
                    continue;
                }
                if block == WATER || block == STILL_WATER {
                    has_water = true;
                    highest_block = false;
                    y -= 1;
                    continue;
                }
                // 水下方五格不放洞穴 免得挖穿河床
                if has_water {
                    y -= 5;
                    has_water = false;
                    continue;
                }

                if array[index(x as i32, z as i32, y)] > 0.35 {
                    if y > 20 {
                        chunk.set_block(x, y as usize, z, AIR);
                    } else {
                        chunk.set_block(x, y as usize, z, LAVA);
                    }
                } else if highest_block {
                    if block == DIRT {
                        chunk.set_block(x, y as usize, z, GRASS);
                    }
                    highest_block = false;
                }

                y -= 1;
            }
        }
    }
}
