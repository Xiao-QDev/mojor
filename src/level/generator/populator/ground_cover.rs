// 对齐 PopulatorGroundCover：按群系把地表换成草/沙/雪等覆盖层
use crate::level::format::chunk::CHUNK_HEIGHT;
use crate::level::generator::biome;
use crate::level::generator::blocks::{AIR, is_solid, is_transparent};
use crate::level::generator::world::GenWorld;

pub fn populate(world: &mut GenWorld, chunk_x: i32, chunk_z: i32) {
    // 覆盖层只落在本区块内 直接借一次区块 否则逐格都要做一次 HashMap 查找
    let Some(chunk) = world.chunk_mut(chunk_x, chunk_z) else {
        return;
    };

    for x in 0..16usize {
        for z in 0..16usize {
            let cover = biome::biome(chunk.biome_id(x, z)).ground_cover;
            if cover.is_empty() {
                continue;
            }

            // 雪层这类非固体方块要往上让一格
            let diff_y = if is_solid(cover[0]) { 0 } else { 1 };

            let mut y = CHUNK_HEIGHT as i32 - 1;
            while y > 0 {
                let id = chunk.block(x, y as usize, z);
                if id != AIR && !is_transparent(id) {
                    break;
                }
                y -= 1;
            }

            let start_y = (y + diff_y).min(CHUNK_HEIGHT as i32 - 1);
            let end_y = start_y - cover.len() as i32;
            let mut yy = start_y;
            while yy > end_y && yy >= 0 {
                let block = cover[(start_y - yy) as usize];
                // 原本是空气又要放固体方块 说明这里是水面 不再往下铺
                if chunk.block(x, yy as usize, z) == AIR && is_solid(block) {
                    break;
                }
                chunk.set_block(x, yy as usize, z, block);
                yy -= 1;
            }
        }
    }
}
