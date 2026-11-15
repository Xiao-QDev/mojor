// 生成期的区块视图 对齐 Nukkit 的 ChunkManager 接口
// 生成器可以通过它跨区块读写 装饰物长在区块边界上也不会被截断
use std::collections::HashMap;

use crate::level::format::chunk::{CHUNK_HEIGHT, Chunk};

pub struct GenWorld<'a> {
    chunks: &'a mut HashMap<(i32, i32), Chunk>,
    // 本 region 的区块范围 越界的读写直接丢掉
    bounds: Option<((i32, i32), (i32, i32))>,
    // 只读地形层：chunks 里没有的区块先克隆一份再写（区块级写时复制）。
    // 每个待装饰区块都用一份干净的视图，装饰结果与调用顺序、邻居装饰无关。
    base: Option<&'a HashMap<(i32, i32), Chunk>>,
}

impl<'a> GenWorld<'a> {
    pub fn new(chunks: &'a mut HashMap<(i32, i32), Chunk>) -> Self {
        Self {
            chunks,
            bounds: None,
            base: None,
        }
    }

    /// 写时复制视图：读取先看本层写入、再看只读地形；写入时把整个区块
    /// 从地形克隆过来。装饰一个区块前调用 [`Self::reset_overlay`] 丢掉
    /// 上一次的写入，就能保证每次都从纯地形开始。
    pub fn with_base(
        chunks: &'a mut HashMap<(i32, i32), Chunk>,
        base: &'a HashMap<(i32, i32), Chunk>,
    ) -> Self {
        Self {
            chunks,
            bounds: None,
            base: Some(base),
        }
    }

    /// 丢掉本视图所有写入，回到纯地形。
    pub fn reset_overlay(&mut self) {
        self.chunks.clear();
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

    fn find_chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Chunk> {
        if !self.in_bounds(chunk_x, chunk_z) {
            return None;
        }
        self.chunks
            .get(&(chunk_x, chunk_z))
            .or_else(|| self.base.and_then(|base| base.get(&(chunk_x, chunk_z))))
    }

    fn materialize(&mut self, chunk_x: i32, chunk_z: i32) -> Option<&mut Chunk> {
        if !self.in_bounds(chunk_x, chunk_z) {
            return None;
        }
        if !self.chunks.contains_key(&(chunk_x, chunk_z)) {
            let source = self.base.and_then(|base| base.get(&(chunk_x, chunk_z)))?;
            self.chunks.insert((chunk_x, chunk_z), source.clone());
        }
        self.chunks.get_mut(&(chunk_x, chunk_z))
    }

    pub fn contains(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.find_chunk(chunk_x, chunk_z).is_some()
    }

    pub fn chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Chunk> {
        self.find_chunk(chunk_x, chunk_z)
    }

    pub fn chunk_mut(&mut self, chunk_x: i32, chunk_z: i32) -> Option<&mut Chunk> {
        self.materialize(chunk_x, chunk_z)
    }

    pub fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.find_chunk(x >> 4, z >> 4)
            .map(|chunk| chunk.block((x & 0xf) as usize, y as usize, (z & 0xf) as usize))
            .unwrap_or(0)
    }

    // 区块不存在或高度越界时静默丢弃 与 Nukkit 的 setBlockIdAt 一致
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u8) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        if let Some(chunk) = self.materialize(x >> 4, z >> 4) {
            chunk.set_block((x & 0xf) as usize, y as usize, (z & 0xf) as usize, id);
        }
    }

    pub fn meta(&self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.find_chunk(x >> 4, z >> 4)
            .map(|chunk| chunk.meta((x & 0xf) as usize, y as usize, (z & 0xf) as usize))
            .unwrap_or(0)
    }

    pub fn set_meta(&mut self, x: i32, y: i32, z: i32, data: u8) {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return;
        }
        if let Some(chunk) = self.materialize(x >> 4, z >> 4) {
            chunk.set_meta((x & 0xf) as usize, y as usize, (z & 0xf) as usize, data);
        }
    }

    pub fn set_block_with_meta(&mut self, x: i32, y: i32, z: i32, id: u8, data: u8) {
        self.set_block(x, y, z, id);
        if data != 0 {
            self.set_meta(x, y, z, data);
        }
    }

    // RTG 内部统一用 state = id << 4 | meta 记法，population 阶段直接用它读写
    pub fn block_state(&self, x: i32, y: i32, z: i32) -> i32 {
        (self.block(x, y, z) as i32) << 4 | self.meta(x, y, z) as i32
    }

    pub fn set_block_state(&mut self, x: i32, y: i32, z: i32, state: i32) -> bool {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return false;
        }
        if self.block_state(x, y, z) == state {
            return false;
        }
        let Some(chunk) = self.materialize(x >> 4, z >> 4) else {
            return false;
        };
        let (lx, ly, lz) = ((x & 0xf) as usize, y as usize, (z & 0xf) as usize);
        chunk.set_block(lx, ly, lz, (state >> 4) as u8);
        chunk.set_meta(lx, ly, lz, (state & 0xf) as u8);
        true
    }

    pub fn is_air(&self, x: i32, y: i32, z: i32) -> bool {
        self.block(x, y, z) == 0
    }

    // 对应 RTG 的 `isBlockLoaded`：只看区块在不在，不看 y
    pub fn is_block_loaded(&self, x: i32, z: i32) -> bool {
        self.contains(x >> 4, z >> 4)
    }

    // 生成期没有真实光照，按"露天"处理（与 RTG 在明亮地表的行为一致）
    pub fn light(&self, _x: i32, _y: i32, _z: i32) -> u8 {
        15
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

    // 写时复制：没写过的区块读地形，写过之后读自己的副本，地形层不被污染。
    #[test]
    fn copy_on_write_view_starts_from_terrain() {
        let mut terrain = HashMap::new();
        let mut origin = Chunk::empty(0, 0);
        origin.set_block(1, 10, 1, 2);
        terrain.insert((0, 0), origin);

        let mut working = HashMap::new();
        let mut world = GenWorld::with_base(&mut working, &terrain);
        assert_eq!(world.block(1, 10, 1), 2, "没写过时读到地形");
        world.set_block(1, 11, 1, 17);
        assert_eq!(world.block(1, 11, 1), 17, "写过之后读到自己的副本");
        assert_eq!(terrain[&(0, 0)].block(1, 11, 1), 0, "地形层不能被污染");

        world.reset_overlay();
        assert_eq!(world.block(1, 11, 1), 0, "下一次装饰从纯地形开始");
        assert_eq!(world.block(1, 10, 1), 2);
    }
}
