// 天空光照 按列自上而下传播
// 与 Nukkit 的两处差别：地表方块自身给满 15，透明方块（水 树叶 冰 植物）不中断传播
use crate::level::format::chunk::{CHUNK_HEIGHT, CHUNK_WIDTH, Chunk};
use crate::level::generator::blocks::is_transparent;

pub const FULL_LIGHT: u8 = 15;

// 重算高度图与天空光 区块生成或装饰改动后调用
pub fn rebuild(chunk: &mut Chunk) {
    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            rebuild_column(chunk, x, z);
        }
    }
}

// 只重算一列 光照按列垂直传播 改一格不会影响别的列
pub fn rebuild_column(chunk: &mut Chunk, x: usize, z: usize) {
    let mut top = 0;
    for y in (0..CHUNK_HEIGHT).rev() {
        if chunk.block(x, y, z) != 0 {
            top = y;
            break;
        }
    }
    chunk.set_height(x, z, top as u8);

    // 先给当前格光照再判断遮挡 地表方块自身也要亮
    let mut lit = true;
    for y in (0..CHUNK_HEIGHT).rev() {
        if lit {
            chunk.set_sky_light(x, y, z, FULL_LIGHT);
            if !is_transparent(chunk.block(x, y, z)) {
                lit = false;
            }
        } else {
            chunk.set_sky_light(x, y, z, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::blocks::{DIRT, GRASS, LEAVES, STONE, WATER};

    #[test]
    fn surface_itself_is_lit() {
        let mut chunk = Chunk::empty(0, 0);
        for x in 0..CHUNK_WIDTH {
            for z in 0..CHUNK_WIDTH {
                chunk.set_block(x, 0, z, STONE);
                chunk.set_block(x, 1, z, DIRT);
                chunk.set_block(x, 2, z, GRASS);
            }
        }
        rebuild(&mut chunk);

        assert_eq!(chunk.height(3, 3), 2);
        assert_eq!(chunk.sky_light(3, CHUNK_HEIGHT - 1, 3), FULL_LIGHT);
        assert_eq!(chunk.sky_light(3, 3, 3), FULL_LIGHT);
        // 地表方块自己亮 它下面立刻转暗
        assert_eq!(chunk.sky_light(3, 2, 3), FULL_LIGHT);
        assert_eq!(chunk.sky_light(3, 1, 3), 0);
        assert_eq!(chunk.sky_light(3, 0, 3), 0);
    }

    // 水 树叶 这类透明方块不挡光 石头自身亮 它下面才暗
    #[test]
    fn transparent_blocks_pass_light() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_block(0, 1, 0, STONE);
        chunk.set_block(0, 2, 0, WATER);
        chunk.set_block(0, 3, 0, LEAVES);
        rebuild(&mut chunk);

        assert_eq!(chunk.height(0, 0), 3);
        assert_eq!(chunk.sky_light(0, 3, 0), FULL_LIGHT);
        assert_eq!(chunk.sky_light(0, 2, 0), FULL_LIGHT);
        assert_eq!(chunk.sky_light(0, 1, 0), FULL_LIGHT);
        assert_eq!(chunk.sky_light(0, 0, 0), 0);
    }

    // 空气柱整根都亮 高度图记 0
    #[test]
    fn empty_column_is_fully_lit() {
        let mut chunk = Chunk::empty(0, 0);
        rebuild(&mut chunk);
        assert_eq!(chunk.height(0, 0), 0);
        for y in 0..CHUNK_HEIGHT {
            assert_eq!(chunk.sky_light(0, y, 0), FULL_LIGHT);
        }
    }
}
