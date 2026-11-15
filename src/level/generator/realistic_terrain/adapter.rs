//! 把 RTG 的 `ChunkPrimer` 落到 MCPE 0.14.3 的 `Chunk`。
//!
//! 两者的方块记法不同：
//! - `ChunkPrimer` 用 RTG 的 `state = id << 4 | meta`（256 高）
//! - `Chunk` 分 `id` / `meta` 两个数组（128 高）
//!
//! 高度已经在 `fit_world_height` 里压到 127 以内，这里拆包、拷贝，并把 Java
//! state 映射成 Bedrock native state（例如 Java 灰化土 `50` → native `243`）。
//! `ChunkPrimer` 是 terrain/surface 的 Java state 唯一出口，映射只在这一处做，
//! 后续 populate 里的结构 placer 已经直接读写 native state。
//! 高度图按 `light::rebuild` 的口径写（最高非空气方块的 y），免得区块
//! 在 populate 之前下发时高度图全是 0。

use crate::level::format::chunk::{CHUNK_HEIGHT, Chunk};
use crate::level::generator::biome;
use crate::level::generator::realistic_terrain::chunk_primer::ChunkPrimer;
use crate::level::generator::realistic_terrain::native_block_mapper::to_native;

/// `biomes` 是海岸抖动之后的最终 biome（`[x][z]`），`TerrainGenerator` 顺手带出来的。
pub fn to_chunk(
    primer: &ChunkPrimer,
    biomes: &[[u8; 16]; 16],
    chunk_x: i32,
    chunk_z: i32,
) -> Chunk {
    let mut chunk = Chunk::empty(chunk_x, chunk_z);

    for (x, biome_row) in biomes.iter().enumerate() {
        for (z, &biome_id) in biome_row.iter().enumerate() {
            let mut top = 0u8;
            for y in 0..CHUNK_HEIGHT {
                let state = to_native(primer.get_block_state(x as i32, y as i32, z as i32));
                if state == 0 {
                    continue;
                }
                chunk.set_block(x, y, z, (state >> 4) as u8);
                chunk.set_meta(x, y, z, (state & 0xf) as u8);
                top = y as u8;
            }
            chunk.set_height(x, z, top);
            chunk.set_biome(x, z, biome_id, biome::biome(biome_id).color);
        }
    }

    chunk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::realistic_terrain::terrain_generator::{
        TerrainGenerator, fit_world_height,
    };

    #[test]
    fn copies_ids_and_meta() {
        let mut generator = TerrainGenerator::new(1234);
        let (primer, biomes) = generator.generate_fitted(0, 0);
        let chunk = to_chunk(&primer, &biomes, 0, 0);

        for x in 0..16usize {
            for z in 0..16usize {
                let mut top = 0u8;
                for y in 0..CHUNK_HEIGHT {
                    let state = to_native(primer.get_block_state(x as i32, y as i32, z as i32));
                    assert_eq!(chunk.block(x, y, z), (state >> 4) as u8);
                    assert_eq!(chunk.meta(x, y, z), (state & 0xf) as u8);
                    if state != 0 {
                        top = y as u8;
                    }
                }
                assert_eq!(chunk.height(x, z), top);
                assert_eq!(chunk.biome_id(x, z), biomes[x][z]);
            }
        }
    }

    #[test]
    fn converts_java_podzol_to_native_id() {
        let mut primer = ChunkPrimer::new(&[0.0; 256]);
        // Java 灰化土：dirt(3) 的 variant 2，state = (3 << 4) | 2
        primer.set_block_state(3, 100, 4, (3 << 4) | 2);
        let biomes = [[5u8; 16]; 16];
        let chunk = to_chunk(&primer, &biomes, 0, 0);

        assert_eq!(chunk.block(3, 100, 4), 243);
        assert_eq!(chunk.meta(3, 100, 4), 0);
        assert_eq!(chunk.height(3, 4), 100);
    }

    #[test]
    fn fit_keeps_low_ground_and_caps_peaks() {
        // 海平面和丘陵原样保留
        assert_eq!(fit_world_height(0.0), 0.0);
        assert_eq!(fit_world_height(63.0), 63.0);
        assert_eq!(fit_world_height(96.0), 96.0);
        // 上限压到 127，且保持单调
        assert!((fit_world_height(175.0) - 127.0).abs() < 1e-9);
        assert!(fit_world_height(140.0) > 96.0);
        assert!(fit_world_height(140.0) < fit_world_height(160.0));
    }
}
