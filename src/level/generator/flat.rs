use crate::level::format::chunk::Chunk;

pub const BEDROCK: u8 = 7;
pub const DIRT: u8 = 3;
pub const GRASS: u8 = 2;

// 平坦世界 基岩 + 3 层泥土 + 草方块 地表 y = 4
pub const SURFACE_Y: u8 = 4;

pub fn generate(chunk_x: i32, chunk_z: i32) -> Chunk {
    let mut chunk = Chunk::empty(chunk_x, chunk_z);

    for x in 0..16 {
        for z in 0..16 {
            chunk.set_block(x, 0, z, BEDROCK);
            chunk.set_block(x, 1, z, DIRT);
            chunk.set_block(x, 2, z, DIRT);
            chunk.set_block(x, 3, z, DIRT);
            chunk.set_block(x, 4, z, GRASS);
            chunk.set_height(x, z, SURFACE_Y);
        }
    }

    chunk
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::format::chunk::{BLOCK_COUNT, HEIGHTMAP_COUNT, NIBBLE_COUNT};

    #[test]
    fn serialized_size_matches_0_14() {
        let data = generate(8, -3).serialize();
        // 32768 方块 + 3 组 nibble + 256 高度 + 256 群系色 + 4 额外数据长度
        assert_eq!(
            data.len(),
            BLOCK_COUNT + NIBBLE_COUNT * 3 + HEIGHTMAP_COUNT + HEIGHTMAP_COUNT * 4 + 4
        );
    }

    #[test]
    fn flat_chunk_layers() {
        let chunk = generate(0, 0);
        assert_eq!(chunk.block(0, 0, 0), BEDROCK);
        assert_eq!(chunk.block(5, 3, 7), DIRT);
        assert_eq!(chunk.block(15, 4, 15), GRASS);
        assert_eq!(chunk.block(8, 5, 8), 0);
    }
}
