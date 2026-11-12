use bytes::Bytes;

use crate::utils::binary::BinaryWriter;

pub const CHUNK_WIDTH: usize = 16;
pub const CHUNK_HEIGHT: usize = 128;
pub const BLOCK_COUNT: usize = CHUNK_WIDTH * CHUNK_WIDTH * CHUNK_HEIGHT;
pub const NIBBLE_COUNT: usize = BLOCK_COUNT / 2;
pub const HEIGHTMAP_COUNT: usize = CHUNK_WIDTH * CHUNK_WIDTH;

// 0.14 的区块是 16x16x128 一整块 索引为 (x << 11) | (z << 7) | y
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    blocks: Box<[u8]>,
    meta: Box<[u8]>,
    sky_light: Box<[u8]>,
    block_light: Box<[u8]>,
    height_map: Box<[u8]>,
    biome_colors: Box<[i32]>,
    // 生成与装饰状态 对齐 FullChunk 的 isGenerated/isPopulated 不参与序列化
    pub generated: bool,
    pub populated: bool,
}

impl Chunk {
    pub fn empty(x: i32, z: i32) -> Self {
        Self {
            x,
            z,
            blocks: vec![0; BLOCK_COUNT].into_boxed_slice(),
            meta: vec![0; NIBBLE_COUNT].into_boxed_slice(),
            // 生成阶段初始全暗 装饰结束后再由光照重算填亮
            sky_light: vec![0; NIBBLE_COUNT].into_boxed_slice(),
            block_light: vec![0; NIBBLE_COUNT].into_boxed_slice(),
            height_map: vec![0; HEIGHTMAP_COUNT].into_boxed_slice(),
            biome_colors: vec![0x4c9b6c; HEIGHTMAP_COUNT].into_boxed_slice(),
            generated: false,
            populated: false,
        }
    }

    // 存档编码要用到底层数组 索引与区块内部布局一致 (x << 11) | (z << 7) | y
    pub fn parts(&self) -> (&[u8], &[u8], &[u8], &[u8]) {
        (&self.blocks, &self.meta, &self.sky_light, &self.block_light)
    }

    pub fn parts_mut(&mut self) -> (&mut [u8], &mut [u8], &mut [u8], &mut [u8]) {
        (
            &mut self.blocks,
            &mut self.meta,
            &mut self.sky_light,
            &mut self.block_light,
        )
    }

    // 天空光整片填充 解码时先铺一层默认值再覆盖
    pub fn fill_sky_light(&mut self, level: u8) {
        let level = level & 0x0f;
        self.sky_light.fill(level * 0x11);
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, id: u8) {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            self.blocks[(x << 11) | (z << 7) | y] = id;
        }
    }

    pub fn block(&self, x: usize, y: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            self.blocks[(x << 11) | (z << 7) | y]
        } else {
            0
        }
    }

    pub fn meta(&self, x: usize, y: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.meta[index] & 0x0f
            } else {
                self.meta[index] >> 4
            }
        } else {
            0
        }
    }

    pub fn set_meta(&mut self, x: usize, y: usize, z: usize, data: u8) {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.meta[index] = (self.meta[index] & 0xf0) | (data & 0x0f);
            } else {
                self.meta[index] = ((data & 0x0f) << 4) | (self.meta[index] & 0x0f);
            }
        }
    }

    // 天空光照 4bit 每格 0 全黑 15 全亮
    pub fn set_sky_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.sky_light[index] = (self.sky_light[index] & 0xf0) | (level & 0x0f);
            } else {
                self.sky_light[index] = ((level & 0x0f) << 4) | (self.sky_light[index] & 0x0f);
            }
        }
    }

    pub fn sky_light(&self, x: usize, y: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.sky_light[index] & 0x0f
            } else {
                self.sky_light[index] >> 4
            }
        } else {
            0
        }
    }

    // 方块光照 主要来自火把一类光源 目前只在导入外部存档时用到
    pub fn set_block_light(&mut self, x: usize, y: usize, z: usize, level: u8) {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.block_light[index] = (self.block_light[index] & 0xf0) | (level & 0x0f);
            } else {
                self.block_light[index] = ((level & 0x0f) << 4) | (self.block_light[index] & 0x0f);
            }
        }
    }

    pub fn block_light(&self, x: usize, y: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && y < CHUNK_HEIGHT && z < CHUNK_WIDTH {
            let index = (x << 10) | (z << 6) | (y >> 1);
            if y & 1 == 0 {
                self.block_light[index] & 0x0f
            } else {
                self.block_light[index] >> 4
            }
        } else {
            0
        }
    }

    // 高度图索引与 Nukkit 一致 是 z 优先
    pub fn set_height(&mut self, x: usize, z: usize, height: u8) {
        if x < CHUNK_WIDTH && z < CHUNK_WIDTH {
            self.height_map[(z << 4) | x] = height;
        }
    }

    pub fn height(&self, x: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && z < CHUNK_WIDTH {
            self.height_map[(z << 4) | x]
        } else {
            0
        }
    }

    // 存档编码按 (z << 4) | x 顺序整块搬 与内部布局一致
    pub fn height_map(&self) -> &[u8] {
        &self.height_map
    }

    pub fn set_height_map(&mut self, data: &[u8]) {
        let len = self.height_map.len().min(data.len());
        self.height_map[..len].copy_from_slice(&data[..len]);
    }

    // 群系 id 存在颜色值的高 8 位 与 BaseFullChunk 的存取方式一致
    pub fn biome_id(&self, x: usize, z: usize) -> u8 {
        if x < CHUNK_WIDTH && z < CHUNK_WIDTH {
            ((self.biome_colors[(z << 4) | x] >> 24) & 0xff) as u8
        } else {
            0
        }
    }

    pub fn set_biome_id(&mut self, x: usize, z: usize, id: u8) {
        if x < CHUNK_WIDTH && z < CHUNK_WIDTH {
            let index = (z << 4) | x;
            self.biome_colors[index] = (self.biome_colors[index] & 0x00ff_ffff) | ((id as i32) << 24);
        }
    }

    pub fn set_biome_color(&mut self, x: usize, z: usize, color: i32) {
        if x < CHUNK_WIDTH && z < CHUNK_WIDTH {
            let index = (z << 4) | x;
            self.biome_colors[index] = (self.biome_colors[index] & !0x00ff_ffff) | (color & 0x00ff_ffff);
        }
    }

    pub fn set_biome(&mut self, x: usize, z: usize, id: u8, color: i32) {
        self.set_biome_id(x, z, id);
        self.set_biome_color(x, z, color);
    }

    // 从存档字节还原区块 长度不对说明数据损坏
    pub fn deserialize(x: i32, z: i32, data: &[u8]) -> Option<Self> {
        let expected =
            BLOCK_COUNT + NIBBLE_COUNT * 3 + HEIGHTMAP_COUNT + HEIGHTMAP_COUNT * 4 + 4;
        if data.len() != expected {
            return None;
        }

        let blocks = data[..BLOCK_COUNT].to_vec().into_boxed_slice();
        let mut offset = BLOCK_COUNT;
        let meta = data[offset..offset + NIBBLE_COUNT].to_vec().into_boxed_slice();
        offset += NIBBLE_COUNT;
        let sky_light = data[offset..offset + NIBBLE_COUNT]
            .to_vec()
            .into_boxed_slice();
        offset += NIBBLE_COUNT;
        let block_light = data[offset..offset + NIBBLE_COUNT]
            .to_vec()
            .into_boxed_slice();
        offset += NIBBLE_COUNT;
        let height_map = data[offset..offset + HEIGHTMAP_COUNT]
            .to_vec()
            .into_boxed_slice();
        offset += HEIGHTMAP_COUNT;

        let mut biome_colors = Vec::with_capacity(HEIGHTMAP_COUNT);
        for _ in 0..HEIGHTMAP_COUNT {
            let color = i32::from_be_bytes(data[offset..offset + 4].try_into().ok()?);
            biome_colors.push(color);
            offset += 4;
        }

        Some(Self {
            x,
            z,
            blocks,
            meta,
            sky_light,
            block_light,
            height_map,
            biome_colors: biome_colors.into_boxed_slice(),
            // 存档里的区块已经生成并装饰过
            generated: true,
            populated: true,
        })
    }

    // 序列化结果每次现算 不在区块对象里常驻缓存
    // 对齐 vanilla 不长期持有序列化副本 每块省掉一份约 83KB 的重复内存
    pub fn payload(&self) -> Bytes {
        self.serialize()
    }

    // 与 Nukkit 0.14 LevelDB 的发送顺序一致
    pub fn serialize(&self) -> Bytes {
        let capacity =
            BLOCK_COUNT + NIBBLE_COUNT * 3 + HEIGHTMAP_COUNT + HEIGHTMAP_COUNT * 4 + 4;
        let mut w = BinaryWriter::with_capacity(capacity);
        w.put_bytes(&self.blocks);
        w.put_bytes(&self.meta);
        w.put_bytes(&self.sky_light);
        w.put_bytes(&self.block_light);
        w.put_bytes(&self.height_map);
        for color in self.biome_colors.iter() {
            w.put_i32(*color);
        }
        // 方块额外数据数量 0
        w.put_lint(0);
        Bytes::from(w.freeze())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_layout_matches_nukkit() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_block(1, 2, 3, 5);
        // (1 << 11) | (3 << 7) | 2 = 2048 + 384 + 2
        assert_eq!(&chunk.blocks[2434], &5);
        assert_eq!(chunk.block(1, 2, 3), 5);
    }

    #[test]
    fn serialized_size_and_layout() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_block(0, 0, 0, 7);
        chunk.set_height(0, 0, 4);
        let data = chunk.serialize();

        assert_eq!(
            data.len(),
            BLOCK_COUNT + NIBBLE_COUNT * 3 + HEIGHTMAP_COUNT + HEIGHTMAP_COUNT * 4 + 4
        );
        assert_eq!(data[0], 7);
        assert_eq!(data[BLOCK_COUNT], 0);
        assert_eq!(data[BLOCK_COUNT + NIBBLE_COUNT * 3], 4);
        assert_eq!(&data[data.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn meta_packs_two_blocks_per_byte() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_meta(0, 0, 0, 3);
        chunk.set_meta(0, 1, 0, 9);
        assert_eq!(chunk.meta[0], 0x93);
    }
}
