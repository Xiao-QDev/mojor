// 0.14 区块的存档编码 按 16 格高的 section 分类后再决定存法
// 纯空气段 全同值段 全亮天空光段都不占空间 这是相比 anvil 最大的收益
// 段内半字节布局与区块内部完全一致 收集与回填都是整块 memcpy
use crate::level::format::chunk::{CHUNK_HEIGHT, CHUNK_WIDTH, Chunk};
use crate::level::generator::biome;

pub const BLOB_VERSION: u8 = 1;

const SECTION_SHIFT: usize = 4;
const SECTION_WIDTH: usize = 1 << SECTION_SHIFT;
const SECTIONS: usize = CHUNK_HEIGHT >> SECTION_SHIFT;
const IDS_LEN: usize = SECTION_WIDTH * SECTION_WIDTH * SECTION_WIDTH;
const NIBBLE_LEN: usize = IDS_LEN / 2;
// 一段占 16 格高 半字节数组里对应 8 个字节
const NIBBLE_SECTION: usize = SECTION_WIDTH / 2;

const KIND_SINGLE: u8 = 0;
const KIND_RAW: u8 = 1;

// 段内方块索引 (lx << 8) | (lz << 4) | ly
#[inline]
fn local(lx: usize, lz: usize, ly: usize) -> usize {
    (lx << 8) | (lz << 4) | ly
}

// 区块内部方块索引 (x << 11) | (z << 7) | y
#[inline]
fn global(lx: usize, lz: usize, wy: usize) -> usize {
    (lx << 11) | (lz << 7) | wy
}

// 段内半字节索引 (lx << 7) | (lz << 3) | (ly >> 1)
#[inline]
fn local_nibble(lx: usize, lz: usize) -> usize {
    (lx << 7) | (lz << 3)
}

// 区块内部半字节索引 (x << 10) | (z << 6) | (y >> 1)
#[inline]
fn global_nibble(lx: usize, lz: usize, sy: usize) -> usize {
    (lx << 10) | (lz << 6) | (sy * NIBBLE_SECTION)
}

#[derive(Clone, Copy, Default)]
struct SectionStat {
    all_air: bool,
    single: bool,
    first: u8,
    light_zero: bool,
    meta_zero: bool,
    sky_full: bool,
}

fn empty_stat() -> SectionStat {
    SectionStat {
        all_air: true,
        single: true,
        first: 0,
        light_zero: true,
        meta_zero: true,
        sky_full: true,
    }
}

fn scan_section(blocks: &[u8], meta: &[u8], sky: &[u8], light: &[u8], sy: usize) -> SectionStat {
    let mut stat = empty_stat();

    // 区块内 y 是连续的 一段的 16 层正好是 16 个连续字节
    // 按 x z 走一遍就是 256 次 16 字节比较 不会跨 2048 字节跳
    let first = blocks[global(0, 0, sy * SECTION_WIDTH)];
    stat.first = first;
    let pattern = u64::from_ne_bytes([first; 8]);

    for lx in 0..SECTION_WIDTH {
        for lz in 0..SECTION_WIDTH {
            let base = global(lx, lz, sy * SECTION_WIDTH);
            let low = u64::from_ne_bytes(blocks[base..base + 8].try_into().unwrap());
            let high = u64::from_ne_bytes(blocks[base + 8..base + 16].try_into().unwrap());
            if low != pattern || high != pattern {
                stat.single = false;
            }
            if low != 0 || high != 0 {
                stat.all_air = false;
            }
        }
    }

    // 半字节按 8 字节整块比较 逐格拆包要看 4096 次 这样只看 256 次
    for lx in 0..SECTION_WIDTH {
        if !stat.meta_zero && !stat.sky_full && !stat.light_zero {
            break;
        }
        for lz in 0..SECTION_WIDTH {
            let start = global_nibble(lx, lz, sy);
            if stat.meta_zero
                && u64::from_ne_bytes(meta[start..start + 8].try_into().unwrap()) != 0
            {
                stat.meta_zero = false;
            }
            if stat.sky_full
                && u64::from_ne_bytes(sky[start..start + 8].try_into().unwrap()) != u64::MAX
            {
                stat.sky_full = false;
            }
            if stat.light_zero
                && u64::from_ne_bytes(light[start..start + 8].try_into().unwrap()) != 0
            {
                stat.light_zero = false;
            }
        }
    }

    stat
}

fn gather_ids(blocks: &[u8], sy: usize, out: &mut [u8; IDS_LEN]) {
    for lx in 0..SECTION_WIDTH {
        for lz in 0..SECTION_WIDTH {
            let source = global(lx, lz, sy * SECTION_WIDTH);
            let target = local(lx, lz, 0);
            out[target..target + SECTION_WIDTH]
                .copy_from_slice(&blocks[source..source + SECTION_WIDTH]);
        }
    }
}

// 段内布局与区块一致 直接整块搬
fn gather_nibble(source: &[u8], sy: usize, out: &mut [u8; NIBBLE_LEN]) {
    for lx in 0..SECTION_WIDTH {
        for lz in 0..SECTION_WIDTH {
            let mut chunk = [0u8; NIBBLE_SECTION];
            let start = global_nibble(lx, lz, sy);
            chunk.copy_from_slice(&source[start..start + NIBBLE_SECTION]);
            let target = local_nibble(lx, lz);
            out[target..target + NIBBLE_SECTION].copy_from_slice(&chunk);
        }
    }
}

fn write_ids(blocks: &mut [u8], sy: usize, ids: &[u8; IDS_LEN]) {
    for lx in 0..SECTION_WIDTH {
        for lz in 0..SECTION_WIDTH {
            let target = global(lx, lz, sy * SECTION_WIDTH);
            let source = local(lx, lz, 0);
            blocks[target..target + SECTION_WIDTH]
                .copy_from_slice(&ids[source..source + SECTION_WIDTH]);
        }
    }
}

fn write_nibble(target: &mut [u8], sy: usize, packed: &[u8; NIBBLE_LEN]) {
    for lx in 0..SECTION_WIDTH {
        for lz in 0..SECTION_WIDTH {
            let start = global_nibble(lx, lz, sy);
            let source = local_nibble(lx, lz);
            target[start..start + NIBBLE_SECTION]
                .copy_from_slice(&packed[source..source + NIBBLE_SECTION]);
        }
    }
}

// 一种数值是否整段一致 一致时只存一个字节
fn uniform(source: &[u8; NIBBLE_LEN]) -> Option<u8> {
    let first = source[0];
    source.iter().all(|&value| value == first).then_some(first)
}

pub fn encode(chunk: &Chunk) -> Vec<u8> {
    let (blocks, meta, sky, light) = chunk.parts();

    let mut present = 0u8;
    let mut meta_mask = 0u8;
    let mut sky_mask = 0u8;
    let mut light_mask = 0u8;
    let mut stats = [SectionStat::default(); SECTIONS];

    for sy in 0..SECTIONS {
        let stat = scan_section(blocks, meta, sky, light, sy);
        stats[sy] = stat;
        if !stat.all_air {
            present |= 1 << sy;
        }
        if !stat.meta_zero {
            meta_mask |= 1 << sy;
        }
        if !stat.sky_full {
            sky_mask |= 1 << sy;
        }
        if !stat.light_zero {
            light_mask |= 1 << sy;
        }
    }

    // 一个典型区块只有三到四段有内容 其余全是空气 预分配按 4 段算
    let mut out = Vec::with_capacity(4 * (IDS_LEN + NIBBLE_LEN));
    out.push(BLOB_VERSION);
    out.push(present);
    out.push(meta_mask);
    out.push(sky_mask);
    out.push(light_mask);

    let mut ids = [0u8; IDS_LEN];
    for sy in 0..SECTIONS {
        if present & (1 << sy) == 0 {
            continue;
        }
        let stat = stats[sy];
        if stat.single {
            out.push(KIND_SINGLE);
            out.push(stat.first);
            continue;
        }
        gather_ids(blocks, sy, &mut ids);
        out.push(KIND_RAW);
        out.extend_from_slice(&ids);
    }

    let mut nibbles = [0u8; NIBBLE_LEN];
    for (mask, source) in [(meta_mask, meta), (sky_mask, sky), (light_mask, light)] {
        for sy in 0..SECTIONS {
            if mask & (1 << sy) == 0 {
                continue;
            }
            gather_nibble(source, sy, &mut nibbles);
            match uniform(&nibbles) {
                Some(value) => {
                    out.push(KIND_SINGLE);
                    out.push(value);
                }
                None => {
                    out.push(KIND_RAW);
                    out.extend_from_slice(&nibbles);
                }
            }
        }
    }

    out.extend_from_slice(chunk.height_map());
    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            out.push(chunk.biome_id(x, z));
        }
    }

    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }

    fn bytes(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }
}

// 数据不完整或版本不认识时返回 None 调用方当作区块不存在重新生成
pub fn decode(chunk_x: i32, chunk_z: i32, data: &[u8]) -> Option<Chunk> {
    let mut reader = Reader::new(data);

    if reader.u8()? != BLOB_VERSION {
        return None;
    }
    let present = reader.u8()?;
    let meta_mask = reader.u8()?;
    let sky_mask = reader.u8()?;
    let light_mask = reader.u8()?;

    let mut chunk = Chunk::empty(chunk_x, chunk_z);
    // 空气默认全亮 没单独存的段就是满天空光
    chunk.fill_sky_light(15);

    let mut ids = [0u8; IDS_LEN];
    {
        let (blocks, ..) = chunk.parts_mut();
        for sy in 0..SECTIONS {
            if present & (1 << sy) == 0 {
                continue;
            }
            match reader.u8()? {
                KIND_SINGLE => {
                    let value = reader.u8()?;
                    for ly in 0..SECTION_WIDTH {
                        let wy = sy * SECTION_WIDTH + ly;
                        for lz in 0..SECTION_WIDTH {
                            for lx in 0..SECTION_WIDTH {
                                blocks[global(lx, lz, wy)] = value;
                            }
                        }
                    }
                }
                KIND_RAW => {
                    ids.copy_from_slice(reader.bytes(IDS_LEN)?);
                    write_ids(blocks, sy, &ids);
                }
                _ => return None,
            }
        }
    }

    for (mask, slot) in [(meta_mask, 0u8), (sky_mask, 1u8), (light_mask, 2u8)] {
        if mask == 0 {
            continue;
        }
        let mut packed = [0u8; NIBBLE_LEN];
        for sy in 0..SECTIONS {
            if mask & (1 << sy) == 0 {
                continue;
            }
            match reader.u8()? {
                KIND_SINGLE => packed.fill(reader.u8()?),
                KIND_RAW => packed.copy_from_slice(reader.bytes(NIBBLE_LEN)?),
                _ => return None,
            }
            let (_, meta, sky, light) = chunk.parts_mut();
            let target = match slot {
                0 => meta,
                1 => sky,
                _ => light,
            };
            write_nibble(target, sy, &packed);
        }
    }

    let heights = reader.bytes(CHUNK_WIDTH * CHUNK_WIDTH)?;
    let biomes = reader.bytes(CHUNK_WIDTH * CHUNK_WIDTH)?;

    chunk.set_height_map(heights);
    for z in 0..CHUNK_WIDTH {
        for x in 0..CHUNK_WIDTH {
            let id = biomes[(z << 4) | x];
            chunk.set_biome(x, z, id, biome::biome(id).color);
        }
    }

    chunk.generated = true;
    chunk.populated = true;
    Some(chunk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::blocks::{BEDROCK, GRASS, STONE, WATER};

    fn assert_same(a: &Chunk, b: &Chunk) {
        for y in 0..CHUNK_HEIGHT {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    assert_eq!(a.block(x, y, z), b.block(x, y, z), "block {x},{y},{z}");
                    assert_eq!(a.meta(x, y, z), b.meta(x, y, z), "meta {x},{y},{z}");
                    assert_eq!(a.sky_light(x, y, z), b.sky_light(x, y, z), "sky {x},{y},{z}");
                    assert_eq!(
                        a.block_light(x, y, z),
                        b.block_light(x, y, z),
                        "light {x},{y},{z}"
                    );
                }
            }
        }
        for z in 0..CHUNK_WIDTH {
            for x in 0..CHUNK_WIDTH {
                assert_eq!(a.height(x, z), b.height(x, z));
                assert_eq!(a.biome_id(x, z), b.biome_id(x, z));
            }
        }
    }

    #[test]
    fn empty_chunk_is_tiny() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.fill_sky_light(15);
        let encoded = encode(&chunk);
        // 只有头部 5 字节 + 高度图 + 群系
        assert_eq!(encoded.len(), 5 + 256 + 256);

        let back = decode(0, 0, &encoded).unwrap();
        assert_same(&chunk, &back);
    }

    // 实心段应该退化成单值 长度远小于原始大小
    #[test]
    fn solid_sections_collapse() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.fill_sky_light(15);
        for y in 0..64 {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    chunk.set_block(x, y, z, STONE);
                }
            }
        }
        let encoded = encode(&chunk);
        assert!(encoded.len() < 1024, "实心区压不下来: {}", encoded.len());

        let back = decode(0, 0, &encoded).unwrap();
        assert_same(&chunk, &back);
    }

    #[test]
    fn round_trips_mixed_content() {
        let mut chunk = Chunk::empty(3, -4);
        for z in 0..CHUNK_WIDTH {
            for x in 0..CHUNK_WIDTH {
                chunk.set_block(x, 0, z, BEDROCK);
                chunk.set_block(x, 1, z, STONE);
                chunk.set_block(x, 2, z, GRASS);
                chunk.set_height(x, z, 2);
                chunk.set_biome(x, z, (x + z) as u8 % 30, 0x00_00ff);
            }
        }
        for y in 40..80 {
            chunk.set_block(5, y, 7, WATER);
            chunk.set_sky_light(5, y, 7, 3);
            chunk.set_block_light(5, y, 7, 11);
        }
        chunk.set_meta(5, 50, 7, 9);
        chunk.set_meta(6, 50, 7, 2);
        chunk.set_block(9, 60, 9, BEDROCK);

        let encoded = encode(&chunk);
        let back = decode(3, -4, &encoded).unwrap();
        assert_same(&chunk, &back);
    }

    // 天空光被遮挡的段必须原样还原 不能退化成默认的满亮
    #[test]
    fn shadowed_sky_light_survives() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.fill_sky_light(15);
        for y in 0..80 {
            for x in 0..CHUNK_WIDTH {
                for z in 0..CHUNK_WIDTH {
                    chunk.set_block(x, y, z, STONE);
                }
            }
        }
        chunk.set_sky_light(0, 79, 0, 0);
        chunk.set_sky_light(1, 79, 0, 7);

        let encoded = encode(&chunk);
        let back = decode(0, 0, &encoded).unwrap();
        assert_eq!(back.sky_light(0, 79, 0), 0);
        assert_eq!(back.sky_light(1, 79, 0), 7);
        assert_eq!(back.sky_light(2, 79, 0), 15);
    }

    #[test]
    fn truncated_data_is_rejected() {
        let mut chunk = Chunk::empty(0, 0);
        chunk.set_block(0, 0, 0, STONE);
        let encoded = encode(&chunk);
        for cut in [0, 1, 5, encoded.len() - 1] {
            assert!(decode(0, 0, &encoded[..cut]).is_none(), "截断 {cut} 应当失败");
        }
    }
}
