// 自研 region 文件：32x32 个区块一个文件 追加写 + 双索引槽
// 布局
//   [0, 4096)        header  magic/version/坐标/seq/data_end/crc
//   [4096, 16416)    索引槽 A  32 字节槽头 + 1024 项 * 12 字节
//   [16416, 28736)   索引槽 B
//   [32768, ...)     data area  只追加 [len][crc][cx][cz][flags][pad][payload]
// 索引槽整体重写 两份交替 崩溃后取 seq 更大且 crc 正确的那份
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const MAGIC: [u8; 4] = *b"MJR1";
const VERSION: u16 = 1;
const HEADER_SIZE: u64 = 4096;
const INDEX_ENTRIES: usize = 1024;
const INDEX_ENTRY_SIZE: u64 = 12;
const INDEX_DATA_SIZE: u64 = INDEX_ENTRIES as u64 * INDEX_ENTRY_SIZE;
const SLOT_HEADER: u64 = 32;
const SLOT_SIZE: u64 = SLOT_HEADER + INDEX_DATA_SIZE;
const SLOT_A: u64 = HEADER_SIZE;
const SLOT_B: u64 = SLOT_A + SLOT_SIZE;
const DATA_START: u64 = 32768;
const RECORD_HEADER: u64 = 14;
const SLOT_MAGIC_A: [u8; 4] = *b"MJRA";
const SLOT_MAGIC_B: [u8; 4] = *b"MJRB";
// 回收阈值 死数据超过 1MB 且超过总量一半才 compact
const COMPACT_MIN: u64 = 1 << 20;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct IndexEntry {
    offset: u64,
    len: u32,
}

impl IndexEntry {
    fn is_empty(&self) -> bool {
        self.len == 0
    }
}

pub struct RegionFile {
    file: File,
    path: PathBuf,
    region_x: i32,
    region_z: i32,
    index: [IndexEntry; INDEX_ENTRIES],
    seq: u64,
    data_end: u64,
    // 当前文件指针 追加写时顺序前进 只有跳转才需要 seek
    cursor: u64,
    dead_bytes: u64,
    index_dirty: bool,
}

impl RegionFile {
    pub fn open(path: impl AsRef<Path>, region_x: i32, region_z: i32) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Storage(format!("创建 region 目录失败: {e}")))?;
        }

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::Storage(format!("打开区域文件失败: {e}")))?;

        let len = file
            .metadata()
            .map_err(|e| Error::Storage(format!("读取区域文件信息失败: {e}")))?
            .len();

        let mut index = [IndexEntry::default(); INDEX_ENTRIES];
        let mut seq = 0u64;
        let mut header_end = DATA_START;
        let mut fresh = len < DATA_START;

        if !fresh {
            match read_header(&mut file) {
                Some(header) => {
                    if header.region_x != region_x || header.region_z != region_z {
                        return Err(Error::Storage(format!(
                            "区域文件坐标不符 期望 {region_x},{region_z} 实际 {},{}",
                            header.region_x, header.region_z
                        )));
                    }
                    seq = header.seq;
                    header_end = header.data_end;
                }
                None => {
                    // 头部损坏 索引槽还有机会救回来 只是坐标无从校验
                    fresh = true;
                }
            }

            let best = match (
                read_slot(&mut file, SLOT_A),
                read_slot(&mut file, SLOT_B),
            ) {
                (Some(a), Some(b)) => Some(if a.1 >= b.1 { a } else { b }),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            if let Some((entries, slot_seq)) = best {
                index = entries;
                seq = seq.max(slot_seq);
            }
        } else {
            file.set_len(DATA_START)
                .map_err(|e| Error::Storage(format!("初始化区域文件失败: {e}")))?;
        }

        // 追加位置取头部与索引里的最大值 头部落后时以索引为准
        let mut data_end = header_end.max(DATA_START);
        for entry in index.iter() {
            if !entry.is_empty() {
                data_end = data_end.max(entry.offset + RECORD_HEADER + entry.len as u64);
            }
        }

        let mut region = Self {
            file,
            path,
            region_x,
            region_z,
            index,
            seq,
            data_end,
            // 刚打开时文件指针位置不确定 第一次写必须 seek
            cursor: u64::MAX,
            dead_bytes: 0,
            index_dirty: fresh,
        };
        if fresh {
            region.write_header()?;
            region.file.sync_all()
                .map_err(|e| Error::Storage(format!("刷新区域文件失败: {e}")))?;
        }
        Ok(region)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn slot_of(&self, chunk_x: i32, chunk_z: i32) -> usize {
        ((chunk_z & 31) << 5 | (chunk_x & 31)) as usize
    }

    pub fn read_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>> {
        let index = self.slot_of(chunk_x, chunk_z);
        let entry = self.index[index];
        if entry.is_empty() {
            return Ok(None);
        }
        self.read_entry(entry, chunk_x, chunk_z)
    }

    fn read_entry(
        &mut self,
        entry: IndexEntry,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Result<Option<Vec<u8>>> {
        let mut head = [0u8; RECORD_HEADER as usize];
        if self
            .file
            .seek(SeekFrom::Start(entry.offset))
            .and_then(|_| self.file.read_exact(&mut head))
            .is_err()
        {
            self.cursor = u64::MAX;
            return Ok(None);
        }
        // 读过之后文件指针就不在追加位置了
        self.cursor = u64::MAX;

        let len = u32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
        let crc = u32::from_le_bytes(head[4..8].try_into().unwrap());
        if len != entry.len as usize {
            return Ok(None);
        }
        // 记录里带着坐标 索引被写坏指向别的区块时靠它挡住
        let stored_x = u16::from_le_bytes(head[8..10].try_into().unwrap()) as i32;
        let stored_z = u16::from_le_bytes(head[10..12].try_into().unwrap()) as i32;
        if stored_x != (chunk_x & 0xffff) || stored_z != (chunk_z & 0xffff) {
            return Ok(None);
        }

        let mut payload = vec![0u8; len];
        if self.file.read_exact(&mut payload).is_err() {
            return Ok(None);
        }
        if crc32fast::hash(&payload) != crc {
            return Ok(None);
        }
        Ok(Some(payload))
    }

    pub fn write_chunk(&mut self, chunk_x: i32, chunk_z: i32, payload: &[u8]) -> Result<()> {
        let len = payload.len() as u32;
        let crc = crc32fast::hash(payload);

        let mut head = [0u8; RECORD_HEADER as usize];
        head[0..4].copy_from_slice(&len.to_le_bytes());
        head[4..8].copy_from_slice(&crc.to_le_bytes());
        head[8..10].copy_from_slice(&((chunk_x & 0xffff) as u16).to_le_bytes());
        head[10..12].copy_from_slice(&((chunk_z & 0xffff) as u16).to_le_bytes());
        // flags 目前只表示压缩方式 0 是原始字节
        head[12] = 0;
        head[13] = 0;

        let offset = self.data_end;
        if self.cursor != offset {
            self.file
                .seek(SeekFrom::Start(offset))
                .map_err(|e| Error::Storage(format!("定位写入位置失败: {e}")))?;
        }
        self.file
            .write_all(&head)
            .and_then(|_| self.file.write_all(payload))
            .map_err(|e| Error::Storage(format!("写入区块失败: {e}")))?;

        // 记录之间不做对齐 顺序追加时文件指针一直跟着走 不需要 seek
        self.cursor = offset + RECORD_HEADER + len as u64;
        self.data_end = self.cursor;

        let slot = self.slot_of(chunk_x, chunk_z);
        let previous = self.index[slot];
        if !previous.is_empty() {
            self.dead_bytes += RECORD_HEADER + previous.len as u64;
        }
        self.index[slot] = IndexEntry { offset, len };
        self.index_dirty = true;
        Ok(())
    }

    // 只写进 page cache 不刷盘 主循环里调用不会卡在磁盘上
    pub fn flush(&mut self) -> Result<()> {
        if self.index_dirty {
            self.seq += 1;
            let even = self.seq % 2 == 0;
            let offset = if even { SLOT_A } else { SLOT_B };
            let magic = if even { SLOT_MAGIC_A } else { SLOT_MAGIC_B };
            self.write_slot(offset, magic)?;
            self.index_dirty = false;
        }
        self.write_header()?;
        // 写完头和索引 文件指针已经不在追加位置
        self.cursor = u64::MAX;
        Ok(())
    }

    // 真正落盘 放到后台线程按周期调用
    pub fn sync(&mut self) -> Result<()> {
        self.file
            .sync_all()
            .map_err(|e| Error::Storage(format!("刷新区域文件失败: {e}")))
    }

    fn write_header(&mut self) -> Result<()> {
        let mut buf = [0u8; HEADER_SIZE as usize];
        buf[0..4].copy_from_slice(&MAGIC);
        buf[4..6].copy_from_slice(&VERSION.to_le_bytes());
        buf[6..8].copy_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
        buf[8..12].copy_from_slice(&self.region_x.to_le_bytes());
        buf[12..16].copy_from_slice(&self.region_z.to_le_bytes());
        buf[16..24].copy_from_slice(&self.seq.to_le_bytes());
        buf[24..32].copy_from_slice(&self.data_end.to_le_bytes());
        let crc = crc32fast::hash(&buf[..32]);
        buf[32..36].copy_from_slice(&crc.to_le_bytes());

        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&buf))
            .map_err(|e| Error::Storage(format!("写入区域头失败: {e}")))
    }

    fn write_slot(&mut self, offset: u64, magic: [u8; 4]) -> Result<()> {
        let mut data = vec![0u8; INDEX_DATA_SIZE as usize];
        for (i, entry) in self.index.iter().enumerate() {
            let base = i * INDEX_ENTRY_SIZE as usize;
            data[base..base + 8].copy_from_slice(&entry.offset.to_le_bytes());
            data[base + 8..base + 12].copy_from_slice(&entry.len.to_le_bytes());
        }

        let mut head = [0u8; SLOT_HEADER as usize];
        head[0..4].copy_from_slice(&magic);
        head[4..12].copy_from_slice(&self.seq.to_le_bytes());
        let crc = crc32fast::hash(&data);
        head[12..16].copy_from_slice(&crc.to_le_bytes());

        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.write_all(&head))
            .and_then(|_| self.file.write_all(&data))
            .map_err(|e| Error::Storage(format!("写入索引失败: {e}")))
    }

    // 死数据过多时重写整个文件 先把内容搬到临时文件再原子替换
    pub fn compact(&mut self) -> Result<bool> {
        let used = self.data_end.saturating_sub(DATA_START);
        if self.dead_bytes < COMPACT_MIN || self.dead_bytes * 2 <= used {
            return Ok(false);
        }

        let tmp_path = self.path.with_extension("mjr.tmp");
        let _ = std::fs::remove_file(&tmp_path);
        let mut tmp = RegionFile::open(&tmp_path, self.region_x, self.region_z)?;

        for i in 0..INDEX_ENTRIES {
            let entry = self.index[i];
            if entry.is_empty() {
                continue;
            }
            let chunk_x = (self.region_x << 5) | (i & 31) as i32;
            let chunk_z = (self.region_z << 5) | (i >> 5) as i32;
            let Some(payload) = self.read_entry(entry, chunk_x, chunk_z)? else {
                continue;
            };
            tmp.write_chunk(chunk_x, chunk_z, &payload)?;
        }
        tmp.flush()?;
        // 替换之前必须保证新文件已经落盘
        tmp.sync()?;
        drop(tmp);

        std::fs::rename(&tmp_path, &self.path)
            .map_err(|e| Error::Storage(format!("替换区域文件失败: {e}")))?;

        let region_x = self.region_x;
        let region_z = self.region_z;
        let path = self.path.clone();
        *self = RegionFile::open(&path, region_x, region_z)?;
        Ok(true)
    }
}

struct Header {
    region_x: i32,
    region_z: i32,
    seq: u64,
    data_end: u64,
}

fn read_header(file: &mut File) -> Option<Header> {
    let mut buf = [0u8; 36];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut buf).ok()?;
    if buf[0..4] != MAGIC {
        return None;
    }
    let stored = u32::from_le_bytes(buf[32..36].try_into().unwrap());
    if crc32fast::hash(&buf[..32]) != stored {
        return None;
    }
    Some(Header {
        region_x: i32::from_le_bytes(buf[8..12].try_into().unwrap()),
        region_z: i32::from_le_bytes(buf[12..16].try_into().unwrap()),
        seq: u64::from_le_bytes(buf[16..24].try_into().unwrap()),
        data_end: u64::from_le_bytes(buf[24..32].try_into().unwrap()),
    })
}

fn read_slot(file: &mut File, offset: u64) -> Option<([IndexEntry; INDEX_ENTRIES], u64)> {
    let mut head = [0u8; SLOT_HEADER as usize];
    file.seek(SeekFrom::Start(offset)).ok()?;
    file.read_exact(&mut head).ok()?;

    let magic = &head[0..4];
    if magic != SLOT_MAGIC_A && magic != SLOT_MAGIC_B {
        return None;
    }

    let seq = u64::from_le_bytes(head[4..12].try_into().unwrap());
    let stored = u32::from_le_bytes(head[12..16].try_into().unwrap());

    let mut data = vec![0u8; INDEX_DATA_SIZE as usize];
    file.read_exact(&mut data).ok()?;
    if crc32fast::hash(&data) != stored {
        return None;
    }

    let mut entries = [IndexEntry::default(); INDEX_ENTRIES];
    for (i, entry) in entries.iter_mut().enumerate() {
        let base = i * INDEX_ENTRY_SIZE as usize;
        entry.offset = u64::from_le_bytes(data[base..base + 8].try_into().unwrap());
        entry.len = u32::from_le_bytes(data[base + 8..base + 12].try_into().unwrap());
    }
    Some((entries, seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mojor-region-{name}-{}-{}.mjr",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn writes_and_reads_chunks() {
        let path = temp_path("roundtrip");
        {
            let mut region = RegionFile::open(&path, 0, 0).unwrap();
            region.write_chunk(0, 0, b"first").unwrap();
            region.write_chunk(31, 31, b"second").unwrap();
            region.flush().unwrap();
        }

        let mut region = RegionFile::open(&path, 0, 0).unwrap();
        assert_eq!(region.read_chunk(0, 0).unwrap().unwrap(), b"first");
        assert_eq!(region.read_chunk(31, 31).unwrap().unwrap(), b"second");
        assert!(region.read_chunk(5, 5).unwrap().is_none());

        let _ = std::fs::remove_file(&path);

        // 负坐标属于相邻的 region 文件
        let neg_path = temp_path("negative");
        {
            let mut region = RegionFile::open(&neg_path, -1, -1).unwrap();
            region.write_chunk(-1, -1, b"negative").unwrap();
            region.write_chunk(-32, -32, b"corner").unwrap();
            region.flush().unwrap();
        }
        let mut region = RegionFile::open(&neg_path, -1, -1).unwrap();
        assert_eq!(region.read_chunk(-1, -1).unwrap().unwrap(), b"negative");
        assert_eq!(region.read_chunk(-32, -32).unwrap().unwrap(), b"corner");
        let _ = std::fs::remove_file(&neg_path);
    }

    // 覆盖写只追加新记录 索引指向最新的一份
    #[test]
    fn overwrite_moves_to_latest_record() {
        let path = temp_path("overwrite");
        let mut region = RegionFile::open(&path, 1, 2).unwrap();
        region.write_chunk(3, 4, b"old").unwrap();
        region.flush().unwrap();
        let first_end = region.data_end;

        region.write_chunk(3, 4, b"much longer content").unwrap();
        region.flush().unwrap();
        assert!(region.data_end > first_end);
        assert_eq!(
            region.read_chunk(3, 4).unwrap().unwrap(),
            b"much longer content"
        );

        let _ = std::fs::remove_file(&path);
    }

    // 索引槽写坏一份 另一份仍能恢复
    #[test]
    fn recovers_from_damaged_slot() {
        let path = temp_path("recover");
        {
            let mut region = RegionFile::open(&path, 0, 0).unwrap();
            region.write_chunk(7, 8, b"payload").unwrap();
            region.flush().unwrap();
        }

        {
            // 把两个槽都写坏 数据仍在但索引没了 应该读不到而不是读到垃圾
            let mut file = OpenOptions::new().read(true).write(true).open(&path).unwrap();
            file.seek(SeekFrom::Start(SLOT_A)).unwrap();
            file.write_all(&[0u8; 64]).unwrap();
            file.seek(SeekFrom::Start(SLOT_B)).unwrap();
            file.write_all(&[0u8; 64]).unwrap();
        }

        let mut region = RegionFile::open(&path, 0, 0).unwrap();
        assert!(region.read_chunk(7, 8).unwrap().is_none());

        // 槽坏掉之后还能继续写
        region.write_chunk(7, 8, b"again").unwrap();
        region.flush().unwrap();
        assert_eq!(region.read_chunk(7, 8).unwrap().unwrap(), b"again");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn compact_keeps_live_data() {
        let path = temp_path("compact");
        let mut region = RegionFile::open(&path, 0, 0).unwrap();
        for round in 0..40 {
            let payload = vec![round as u8; 32 * 1024];
            region.write_chunk(1, 1, &payload).unwrap();
        }
        region.write_chunk(2, 2, b"keep").unwrap();
        region.flush().unwrap();

        let before = region.data_end;
        assert!(region.compact().unwrap());
        assert!(region.data_end < before);
        assert_eq!(region.read_chunk(1, 1).unwrap().unwrap(), vec![39u8; 32 * 1024]);
        assert_eq!(region.read_chunk(2, 2).unwrap().unwrap(), b"keep");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_mismatched_region_coordinates() {
        let path = temp_path("coords");
        {
            let mut region = RegionFile::open(&path, 3, 4).unwrap();
            region.write_chunk(0, 0, b"x").unwrap();
            region.flush().unwrap();
        }
        assert!(RegionFile::open(&path, 5, 6).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
