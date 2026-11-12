// region 文件句柄管理 一个文件一把锁 不同区域可以并行读写
// 句柄里带 12KB 索引表 活跃时留在内存 长期不用的由后台刷盘线程回收
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::level::format::mojoregion::region_file::RegionFile;

// 一分钟没被读写过的区域文件就关掉句柄 下次用到再打开
const FILE_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

type RegionHandle = Arc<Mutex<RegionFile>>;

struct Entry {
    handle: RegionHandle,
    last_used: Instant,
}

pub struct RegionStore {
    dir: PathBuf,
    regions: Mutex<HashMap<(i32, i32), Entry>>,
}

impl RegionStore {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(dir.join("region"))
            .map_err(|e| Error::Storage(format!("创建 region 目录失败: {e}")))?;
        Ok(Self {
            dir,
            regions: Mutex::new(HashMap::new()),
        })
    }

    fn path_of(&self, region_x: i32, region_z: i32) -> PathBuf {
        self.dir
            .join("region")
            .join(format!("r.{region_x}.{region_z}.mjr"))
    }

    fn region(&self, region_x: i32, region_z: i32) -> Result<RegionHandle> {
        let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let key = (region_x, region_z);
        if let Some(entry) = regions.get_mut(&key) {
            entry.last_used = now;
            return Ok(entry.handle.clone());
        }
        let path = self.path_of(region_x, region_z);
        let handle = Arc::new(Mutex::new(RegionFile::open(&path, region_x, region_z)?));
        regions.insert(
            key,
            Entry {
                handle: handle.clone(),
                last_used: now,
            },
        );
        Ok(handle)
    }

    pub fn load(&self, chunk_x: i32, chunk_z: i32) -> Result<Option<Vec<u8>>> {
        let region = self.region(chunk_x >> 5, chunk_z >> 5)?;
        let mut file = region.lock().unwrap_or_else(|e| e.into_inner());
        file.read_chunk(chunk_x, chunk_z)
    }

    // 写入一批区块 先按 region 分组 每组只取一次句柄 最后刷一次盘
    pub fn save(&self, batch: &[((i32, i32), Vec<u8>)]) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let mut groups: HashMap<(i32, i32), Vec<(i32, i32, &[u8])>> = HashMap::new();
        for ((chunk_x, chunk_z), payload) in batch {
            groups
                .entry((chunk_x >> 5, chunk_z >> 5))
                .or_default()
                .push((*chunk_x, *chunk_z, payload.as_slice()));
        }

        for ((region_x, region_z), items) in groups {
            let region = self.region(region_x, region_z)?;
            let mut file = region.lock().unwrap_or_else(|e| e.into_inner());
            for (chunk_x, chunk_z, payload) in items {
                file.write_chunk(chunk_x, chunk_z, payload)?;
            }
            file.flush()?;
        }
        Ok(())
    }

    // 顺带回收死数据 由调用方定期触发
    pub fn compact(&self) -> Result<usize> {
        let regions: Vec<RegionHandle> = self.handles();

        let mut compacted = 0;
        for region in regions {
            let mut file = region.lock().unwrap_or_else(|e| e.into_inner());
            if file.compact()? {
                compacted += 1;
            }
        }
        Ok(compacted)
    }

    // 把已经写进 page cache 的内容刷到磁盘 由后台线程周期调用
    // 只锁单个区域文件 与主循环的写入互不阻塞太久
    pub fn sync_all(&self) -> Result<()> {
        for region in self.handles() {
            let mut file = region.lock().unwrap_or_else(|e| e.into_inner());
            file.sync()?;
        }
        Ok(())
    }

    // 关掉长时间没用的文件句柄 返回关掉的数量
    // 别的线程手上还持有 Arc 时不会真的释放 下次访问会重新打开文件
    pub fn evict_idle(&self, max_idle: Duration) -> usize {
        let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let stale: Vec<(i32, i32)> = regions
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.last_used) >= max_idle)
            .map(|(key, _)| *key)
            .collect();
        for key in &stale {
            regions.remove(key);
        }
        stale.len()
    }

    pub fn evict_stale(&self) -> usize {
        self.evict_idle(FILE_IDLE_TIMEOUT)
    }

    // 业务 GC 用玩家视野算出来的可达分片做回收 不在可达集里的尽快关掉
    pub fn evict_unreachable(&self, live: &HashSet<(i32, i32)>, max_idle: Duration) -> usize {
        let mut regions = self.regions.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let stale: Vec<(i32, i32)> = regions
            .iter()
            .filter(|(key, entry)| {
                !live.contains(key) && now.duration_since(entry.last_used) >= max_idle
            })
            .map(|(key, _)| *key)
            .collect();
        for key in &stale {
            regions.remove(key);
        }
        stale.len()
    }

    fn handles(&self) -> Vec<RegionHandle> {
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|entry| entry.handle.clone())
            .collect()
    }

    pub fn close(&self) -> Result<()> {
        for region in self.handles() {
            let mut file = region.lock().unwrap_or_else(|e| e.into_inner());
            file.flush()?;
            file.sync()?;
        }
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        Ok(())
    }

    pub fn region_count(&self) -> usize {
        self.regions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mojor-store-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    // 闲置文件句柄可以回收 回收后重新打开数据还在
    #[test]
    fn idle_handles_are_evicted_and_reopened_on_demand() {
        use crate::level::format::mojoregion::blob::encode;
        use crate::level::format::chunk::Chunk;

        let dir = temp_dir("evict");
        let store = RegionStore::open(&dir).unwrap();
        let mut chunk = Chunk::empty(1, 2);
        chunk.set_block(0, 5, 0, 9);
        store.save(&[((1, 2), encode(&chunk).to_vec())]).unwrap();
        assert_eq!(store.region_count(), 1);

        // 立刻回收不掉 刚访问过
        assert_eq!(store.evict_idle(Duration::from_secs(60)), 0);
        assert_eq!(store.region_count(), 1);

        // 零空闲全部回收
        assert_eq!(store.evict_idle(Duration::ZERO), 1);
        assert_eq!(store.region_count(), 0);

        let back = store.load(1, 2).unwrap().expect("区块应还在");
        let decoded = crate::level::format::mojoregion::blob::decode(1, 2, &back)
            .expect("重新打开的区块应能解码");
        assert_eq!(decoded.block(0, 5, 0), 9);
        assert_eq!(store.region_count(), 1);
    }

    // 可达分片的文件句柄保留 不可达的回收
    #[test]
    fn gc_keeps_live_handles_and_evicts_unreachable_ones() {
        use crate::level::format::chunk::Chunk;
        use crate::level::format::mojoregion::blob;

        let dir = temp_dir("gc-files");
        let store = RegionStore::open(&dir).unwrap();
        let mut a = Chunk::empty(1, 2);
        a.set_block(0, 0, 0, 1);
        let mut b = Chunk::empty(40, 2);
        b.set_block(0, 0, 0, 1);
        store.save(&[((1, 2), blob::encode(&a).to_vec())]).unwrap();
        store.save(&[((40, 2), blob::encode(&b).to_vec())]).unwrap();
        assert_eq!(store.region_count(), 2);

        std::thread::sleep(Duration::from_millis(1));
        let mut live = HashSet::new();
        live.insert((0, 0));
        assert_eq!(store.evict_unreachable(&live, Duration::ZERO), 1);
        assert_eq!(store.region_count(), 1);

        // 可达的那个还在 重新打开的会自动重建
        assert!(store.load(1, 2).unwrap().is_some());
        assert_eq!(store.region_count(), 1);
    }

    #[test]
    fn chunks_in_same_region_share_a_file() {
        let dir = temp_dir("shared");
        {
            let store = RegionStore::open(&dir).unwrap();
            store
                .save(&[
                    ((0, 0), b"a".to_vec()),
                    ((31, 31), b"b".to_vec()),
                    ((32, 0), b"c".to_vec()),
                ])
                .unwrap();
            assert_eq!(store.region_count(), 2);
            store.close().unwrap();
        }

        let files = std::fs::read_dir(dir.join("region")).unwrap().count();
        assert_eq!(files, 2);

        let store = RegionStore::open(&dir).unwrap();
        assert_eq!(store.load(0, 0).unwrap().unwrap(), b"a");
        assert_eq!(store.load(31, 31).unwrap().unwrap(), b"b");
        assert_eq!(store.load(32, 0).unwrap().unwrap(), b"c");
        assert!(store.load(5, 5).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn negative_coordinates_land_in_their_region() {
        let dir = temp_dir("negative");
        let store = RegionStore::open(&dir).unwrap();
        store
            .save(&[((-1, -1), b"neg".to_vec()), ((-33, 4), b"far".to_vec())])
            .unwrap();
        assert_eq!(store.region_count(), 2);
        assert_eq!(store.load(-1, -1).unwrap().unwrap(), b"neg");
        assert_eq!(store.load(-33, 4).unwrap().unwrap(), b"far");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
