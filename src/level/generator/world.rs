// 生成期的区块视图 对齐 Nukkit 的 ChunkManager 接口
// 生成器可以通过它跨区块读写 装饰物长在区块边界上也不会被截断
use std::collections::HashMap;

use crate::level::format::chunk::{CHUNK_HEIGHT, Chunk};

pub struct GenWorld<'a> {
    chunks: &'a mut HashMap<(i32, i32), Chunk>,
    // 本 region 的区块范围 越界的读写直接丢掉
    bounds: Option<((i32, i32), (i32, i32))>,
}

impl<'a> GenWorld<'a> {
    pub fn new(chunks: &'a mut HashMap<(i32, i32), Chunk>) -> Self {
        Self { chunks, bounds: None }
    }

    pub fn with_bounds(mut self, bounds: Option<((i32, i32), (i32, i32))>) -> Self {
        self.bounds = bounds;
        self
    }

    fn in_bounds(&self, chunk_x: i32, chunk_z: i32) -> bool {
        match self.bounds {
            Some(((min_x, min_z), (max_x, max_z))) => {
                (min_x..=max_x).contains(&chunk_x) && (min_z..=max_z).contains(&chunk_z)
            }
            None => true,
        }
    }

    pub fn contains(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.in_bounds(chunk_x, chunk_z) && self.chunks.contains_key(&(chunk_x, chunk_z))
    }

    pub fn chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Chunk> {
        if !self.in_bounds(chunk_x, chunk_z) {
            return None;
        }
        self.chunks.get(&(chunk_x, chunk_z))
    }

    pub fn chunk_mut(&mut self, chunk_x: i32, chunk_z: i32) -> Option<&mut Chunk> {
        if !self.in_bounds(chunk_x, chunk_z) {
            return None;
        }
        self.chunks.get_mut(&(chunk_x, chunk_z))
    }

    pub fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.chunk(x >> 4, z >> 4)
            .map(|chunk| chunk.block((x & 0xf) as usize, y as usize, (z & 0xf) as usize))
            .unwrap_or(0)
    }

    // 区块不存在或高度越界时静默丢弃 与 Nukkit 的 setBlockIdAt 一致
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        if let Some(chunk) = self.chunk_mut(x >> 4, z >> 4) {
            chunk.set_block((x & 0xf) as usize, y as usize, (z & 0xf) as usize, id);
        }
    }

    pub fn meta(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.chunk(x >> 4, z >> 4)
            .map(|chunk| chunk.meta((x & 0xf) as usize, y as usize, (z & 0xf) as usize))
            .unwrap_or(0)
    }

    pub fn set_meta(&mut self, x: i32, y: i32, z: i32, data: u8) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        if let Some(chunk) = self.chunk_mut(x >> 4, z >> 4) {
            chunk.set_meta((x & 0xf) as usize, y as usize, (z & 0xf) as usize, data);
        }
    }

    pub fn set_block_with_meta(&mut self, x: i32, y: i32, z: i32, id: u8, data: u8) {
        self.set_block(x, y, z, id);
        if data != 0 {
            self.set_meta(x, y, z, data);
        }
    }

    pub fn biome(&self, x: i32, z: i32) -> u8 {
        self.chunk(x >> 4, z >> 4)
            .map(|chunk| chunk.biome_id((x & 0xf) as usize, (z & 0xf) as usize))
            .unwrap_or(0)
    }

    pub fn set_biome(&mut self, x: i32, z: i32, id: u8, color: i32) {
        if let Some(chunk) = self.chunk_mut(x >> 4, z >> 4) {
            chunk.set_biome((x & 0xf) as usize, (z & 0xf) as usize, id, color);
        }
    }

    // 对齐 FullChunk.getHighestBlockAt：自上而下第一个非空气
    pub fn highest_block(&self, x: i32, z: i32) -> i32 {
        for y in (0..CHUNK_HEIGHT as i32).rev() {
            if self.block(x, y, z) != 0 {
                return y;
            }
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::blocks::{BEDROCK, GRASS};

    #[test]
    fn reads_and_writes_across_chunks() {
        let mut chunks = HashMap::new();
        chunks.insert((0, 0), Chunk::empty(0, 0));
        chunks.insert((-1, 0), Chunk::empty(-1, 0));
        let mut world = GenWorld::new(&mut chunks);

        world.set_block(-1, 0, 5, BEDROCK);
        world.set_block(0, 4, 5, GRASS);
        assert_eq!(world.block(-1, 0, 5), BEDROCK);
        assert_eq!(world.block(0, 4, 5), GRASS);
        assert_eq!(world.highest_block(0, 5), 4);
    }

    #[test]
    fn missing_chunk_is_silently_ignored() {
        let mut chunks = HashMap::new();
        let mut world = GenWorld::new(&mut chunks);
        world.set_block(100, 10, 100, BEDROCK);
        assert_eq!(world.block(100, 10, 100), 0);
    }
}
