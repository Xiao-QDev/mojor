// 光照任务快照 中心区块 + 周围一圈拼成 48x48x128 的连续数组
// 引擎只在快照上跑 BFS 结束后按区块 diff 出变脏的部分打包回执
// 区块数据在 region 线程抓取（ChunkManager 是 region 私有的） 整体 move 给后台 worker
//
// 缺失的邻居区块按 vanilla"未加载 section 不参与光照"处理：铺满不透光的基岩
// 光照既不传入也不传出该方向 语义与 vanilla 的 storingLightForSection=false 一致
use crate::level::chunk_manager::ChunkManager;
use crate::level::format::chunk::{CHUNK_HEIGHT, CHUNK_WIDTH, NIBBLE_COUNT};

// 每边 48 格 3x3 区块
pub const SNAP_W: usize = 48;
const SNAP_SIZE: usize = SNAP_W * SNAP_W * CHUNK_HEIGHT;

// 两条光照通道 天空光与方块光 队列与存储各自独立
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Channel {
    Sky,
    Block,
}

// 快照内坐标 (x,z) 0..48 y 0..127 的线性索引 y 变化最连续
#[inline]
fn idx(x: usize, y: usize, z: usize) -> usize {
    (x * SNAP_W + z) * CHUNK_HEIGHT + y
}

// 一条光照回执 携带变脏区块的完整光照 nibble
// 布局与 Chunk 的 sky_light/block_light 一致 索引 (x<<10)|(z<<6)|(y>>1)
pub struct ChunkLight {
    pub x: i32,
    pub z: i32,
    pub sky_light: Vec<u8>,
    pub block_light: Vec<u8>,
}

pub struct LightReceipt {
    // 这次任务的目标区块 只有它是全量算过的
    // 回执里其它区块只是在边界上被牵动
    pub owner: (i32, i32),
    pub chunks: Vec<ChunkLight>,
}

pub struct Snapshot {
    pub cx: i32,
    pub cz: i32,
    blocks: Vec<u8>,
    sky: Vec<u8>,
    block: Vec<u8>,
    // 每列的天空源起点 快照坐标系 48x48
    sources: Vec<i16>,
    // 每通道 9 bit bit i = 中心 (i/3-1, i%3-1) 偏移的区块被写过
    dirty: [u16; 2],
}

impl Snapshot {
    // 在 region 线程上抓取快照 只读 ChunkManager 不触发生成
    // 缺失的邻居区块铺基岩挡光 该方向不参与光照传播
    pub fn grab(manager: &ChunkManager, cx: i32, cz: i32) -> Self {
        let mut blocks = vec![16u8; SNAP_SIZE];
        let mut sky = vec![0u8; SNAP_SIZE];
        let mut block = vec![0u8; SNAP_SIZE];

        for sx in 0..SNAP_W {
            for sz in 0..SNAP_W {
                let dx = (sx / CHUNK_WIDTH) as i32 - 1;
                let dz = (sz / CHUNK_WIDTH) as i32 - 1;
                let lx = sx % CHUNK_WIDTH;
                let lz = sz % CHUNK_WIDTH;
                let Some(chunk) = manager.peek_chunk(cx + dx, cz + dz) else {
                    continue;
                };
                let (chunk_blocks, _, chunk_sky, chunk_light) = chunk.parts();
                for y in 0..CHUNK_HEIGHT {
                    let to = idx(sx, y, sz);
                    blocks[to] = chunk_blocks[(lx << 11) | (lz << 7) | y];
                    let sky_byte = chunk_sky[(lx << 10) | (lz << 6) | (y >> 1)];
                    sky[to] = if y & 1 == 0 {
                        sky_byte & 0x0f
                    } else {
                        sky_byte >> 4
                    };
                    let light_byte = chunk_light[(lx << 10) | (lz << 6) | (y >> 1)];
                    block[to] = if y & 1 == 0 {
                        light_byte & 0x0f
                    } else {
                        light_byte >> 4
                    };
                }
            }
        }

        // 源列初值 全部 2304 列现场算 后续 Update 只重算被改动的列
        let mut sources = vec![0i16; SNAP_W * SNAP_W];
        for x in 0..SNAP_W {
            for z in 0..SNAP_W {
                sources[x * SNAP_W + z] =
                    super::sources::column_lowest_source(|y| blocks[idx(x, y as usize, z)]);
            }
        }

        Self {
            cx,
            cz,
            blocks,
            sky,
            block,
            sources,
            dirty: [0, 0],
        }
    }

    // 世界坐标 -> 快照坐标 改动点一定落在中心区块 但保险起见越界返回 None
    pub fn world_to_snap(&self, wx: i32, wz: i32) -> Option<(usize, usize)> {
        let sx = wx - (self.cx - 1) * CHUNK_WIDTH as i32;
        let sz = wz - (self.cz - 1) * CHUNK_WIDTH as i32;
        if (0..SNAP_W as i32).contains(&sx) && (0..SNAP_W as i32).contains(&sz) {
            Some((sx as usize, sz as usize))
        } else {
            None
        }
    }

    #[inline]
    pub fn block(&self, x: usize, y: usize, z: usize) -> u8 {
        self.blocks[idx(x, y, z)]
    }

    #[inline]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, id: u8) {
        self.blocks[idx(x, y, z)] = id;
    }

    #[inline]
    pub fn level(&self, channel: Channel, x: usize, y: usize, z: usize) -> u8 {
        match channel {
            Channel::Sky => self.sky[idx(x, y, z)],
            Channel::Block => self.block[idx(x, y, z)],
        }
    }

    // 写光照 值不变不写也不标脏 保证回执只带真的变了的区块
    #[inline]
    pub fn set_level(&mut self, channel: Channel, x: usize, y: usize, z: usize, level: u8) {
        let slot = match channel {
            Channel::Sky => &mut self.sky,
            Channel::Block => &mut self.block,
        };
        let i = idx(x, y, z);
        if slot[i] != level {
            slot[i] = level;
            self.dirty[channel as usize] |= 1 << chunk_index(x, z);
        }
    }

    #[inline]
    pub fn column_source(&self, x: usize, z: usize) -> i16 {
        self.sources[x * SNAP_W + z]
    }

    #[inline]
    pub fn set_column_source(&mut self, x: usize, z: usize, value: i16) {
        self.sources[x * SNAP_W + z] = value;
    }

    // 四邻列的最低源最大值 vanilla addSourcesAbove 的 neighborLowestSourceY
    // 出快照的列给 i16::MIN 使 y < neighborLowest 永假 该方向不入队
    pub fn neighbor_lowest(&self, x: usize, z: usize) -> i16 {
        let at = |xx: i32, zz: i32| -> i16 {
            if xx < 0 || zz < 0 || xx >= SNAP_W as i32 || zz >= SNAP_W as i32 {
                i16::MIN
            } else {
                self.sources[xx as usize * SNAP_W + zz as usize]
            }
        };
        at(x as i32 - 1, z as i32)
            .max(at(x as i32 + 1, z as i32))
            .max(at(x as i32, z as i32 - 1))
            .max(at(x as i32, z as i32 + 1))
    }

    // 引擎跑完后把变脏区块的光照打包成回执
    // 只打包 dirty 位置命中的区块 全字节压回 nibble 布局
    pub fn into_receipt(self) -> LightReceipt {
        let Self {
            cx,
            cz,
            sky,
            block,
            dirty,
            ..
        } = self;
        let mut chunks = Vec::new();
        // 两条通道的脏区块并集
        let mask = dirty[0] | dirty[1];
        for bit in 0..9u32 {
            if mask & (1 << bit) == 0 {
                continue;
            }
            let dx = (bit / 3) as i32 - 1;
            let dz = (bit % 3) as i32 - 1;
            let base_x = (dx + 1) as usize * CHUNK_WIDTH;
            let base_z = (dz + 1) as usize * CHUNK_WIDTH;

            let mut packed_sky = vec![0u8; NIBBLE_COUNT];
            let mut packed_light = vec![0u8; NIBBLE_COUNT];
            for lx in 0..CHUNK_WIDTH {
                for lz in 0..CHUNK_WIDTH {
                    for y in 0..CHUNK_HEIGHT {
                        let sky_v = sky[idx(base_x + lx, y, base_z + lz)] & 0x0f;
                        let light_v = block[idx(base_x + lx, y, base_z + lz)] & 0x0f;
                        let at = (lx << 10) | (lz << 6) | (y >> 1);
                        if y & 1 == 0 {
                            packed_sky[at] |= sky_v;
                            packed_light[at] |= light_v;
                        } else {
                            packed_sky[at] |= sky_v << 4;
                            packed_light[at] |= light_v << 4;
                        }
                    }
                }
            }
            chunks.push(ChunkLight {
                x: cx + dx,
                z: cz + dz,
                sky_light: packed_sky,
                block_light: packed_light,
            });
        }
        LightReceipt {
            owner: (cx, cz),
            chunks,
        }
    }
}

// 快照坐标 (x,z) -> 3x3 区块序号 bit
#[inline]
fn chunk_index(x: usize, z: usize) -> u32 {
    ((x / CHUNK_WIDTH) * 3 + (z / CHUNK_WIDTH)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    // 全空气的 3x3 快照 源列全开放
    fn empty_snap() -> Snapshot {
        Snapshot {
            cx: 0,
            cz: 0,
            blocks: vec![0; SNAP_SIZE],
            sky: vec![0; SNAP_SIZE],
            block: vec![0; SNAP_SIZE],
            sources: vec![super::super::sources::WORLD_BOTTOM_SOURCE; SNAP_W * SNAP_W],
            dirty: [0, 0],
        }
    }

    #[test]
    fn grab_fills_missing_neighbors_with_bedrock() {
        let manager = ChunkManager::new();
        let snap = Snapshot::grab(&manager, 5, 7);
        // 中心也不在缓存里 全部按缺失处理
        assert_eq!(snap.block(24, 64, 24), 16);
        // 中心坐标对齐 (5,7) 对应快照 (24,24)
        assert_eq!(snap.world_to_snap(5 * 16 + 8, 7 * 16 + 8), Some((24, 24)));
        assert_eq!(snap.world_to_snap(-100, 8), None);
    }

    #[test]
    fn set_level_marks_the_chunk_dirty() {
        let mut snap = empty_snap();
        // 中心区块 bit 4
        snap.set_level(Channel::Sky, 24, 64, 24, 15);
        assert_eq!(snap.dirty[0], 1 << 4);
        // 邻居区块 bit 5（+z 方向）
        snap.set_level(Channel::Sky, 24, 64, 40, 15);
        assert_eq!(snap.dirty[0], (1 << 4) | (1 << 5));
        // 相同值重复写不重复标脏
        snap.set_level(Channel::Sky, 24, 64, 24, 15);
        assert_eq!(snap.dirty[0], (1 << 4) | (1 << 5));
    }

    #[test]
    fn receipt_packs_nibbles_in_chunk_layout() {
        let mut snap = empty_snap();
        snap.set_level(Channel::Block, 16, 10, 16, 9); // 中心区块 (0,0) 本地 (0,10,0)
        let receipt = snap.into_receipt();
        assert_eq!(receipt.chunks.len(), 1);
        let data = &receipt.chunks[0];
        assert_eq!((data.x, data.z), (0, 0));
        // (0<<10)|(0<<6)|(10>>1) = 5 偶数 y 在低位
        assert_eq!(data.block_light[5] & 0x0f, 9);
        assert_eq!(data.sky_light[5], 0);
    }

    #[test]
    fn neighbor_lowest_ignores_out_of_snapshot() {
        let mut snap = empty_snap();
        snap.set_column_source(0, 5, 30);
        // x=1 的西邻是 (0,5) 值 30
        assert_eq!(snap.neighbor_lowest(1, 5), 30);
        // x=0 的西邻出快照给 i16::MIN 但南北邻还是全透列的 -1 取 max 后是 -1
        assert_eq!(snap.neighbor_lowest(0, 5), super::super::sources::WORLD_BOTTOM_SOURCE);
    }
}
