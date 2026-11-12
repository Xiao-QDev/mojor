use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use crate::error::Result;
use crate::level::format::mojoregion::RegionStore;

// 区块存档 一个 region 文件管 32x32 个区块
// 编解码在 format::mojoregion::blob 这里只负责按 region 分发
pub struct ChunkLoader {
    store: RegionStore,
}

impl ChunkLoader {
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let store = RegionStore::open(dir.as_ref())?;
        Ok(Self { store })
    }

    pub fn load(&self, x: i32, z: i32) -> Result<Option<Vec<u8>>> {
        self.store.load(x, z)
    }

    pub fn save(&self, chunks: &[((i32, i32), Vec<u8>)]) -> Result<()> {
        self.store.save(chunks)
    }

    // 把写进 page cache 的内容刷到磁盘 顺带回收闲置的文件句柄
    // 后台线程按周期调用 玩家跑图打开的区域文件不会一直常驻
    pub fn sync(&self) -> Result<()> {
        self.store.sync_all()?;
        self.store.evict_stale();
        Ok(())
    }

    // 业务 GC 回收不在玩家可达分片里的文件句柄
    pub fn gc_files(&self, live: &HashSet<(i32, i32)>) -> usize {
        self.store
            .evict_unreachable(live, Duration::from_secs(2))
    }

    pub fn compact(&self) -> Result<usize> {
        self.store.compact()
    }

    pub fn close(&self) -> Result<()> {
        self.store.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use crate::level::format::mojoregion::blob;
    use crate::level::generator::blocks::STONE;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mojor-loader-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn sample(x: i32, z: i32) -> Vec<u8> {
        let mut chunk = Chunk::empty(x, z);
        chunk.set_block(0, 0, 0, STONE);
        blob::encode(&chunk)
    }

    #[test]
    fn saves_and_loads_chunk() {
        let dir = temp_dir("roundtrip");
        {
            let storage = ChunkLoader::open(&dir).unwrap();
            storage
                .save(&[((0, 0), sample(0, 0)), ((-1, 2), sample(-1, 2))])
                .unwrap();
        }

        let storage = ChunkLoader::open(&dir).unwrap();
        let first = blob::decode(0, 0, &storage.load(0, 0).unwrap().unwrap()).unwrap();
        let second = blob::decode(-1, 2, &storage.load(-1, 2).unwrap().unwrap()).unwrap();
        assert_eq!(first.block(0, 0, 0), STONE);
        assert_eq!(second.block(0, 0, 0), STONE);
        assert_eq!(storage.load(9, 9).unwrap(), None);

        drop(storage);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 后写的覆盖先写的
    #[test]
    fn save_overwrites_existing_chunk() {
        let dir = temp_dir("overwrite");
        let storage = ChunkLoader::open(&dir).unwrap();
        storage.save(&[((3, 4), sample(3, 4))]).unwrap();

        let mut edited = Chunk::empty(3, 4);
        edited.set_block(0, 1, 0, STONE);
        storage
            .save(&[((3, 4), blob::encode(&edited))])
            .unwrap();

        let back = blob::decode(3, 4, &storage.load(3, 4).unwrap().unwrap()).unwrap();
        assert_eq!(back.block(0, 1, 0), STONE);

        drop(storage);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // 真实地形编码后应该远小于原始大小
    #[test]
    fn generated_chunk_compresses_well() {
        use crate::level::generator::{Generator, world::GenWorld};
        use std::collections::HashMap;

        let mut chunks = HashMap::new();
        chunks.insert((0, 0), Chunk::empty(0, 0));
        {
            let mut world = GenWorld::new(&mut chunks);
            let mut generator = Generator::terrain(1337);
            generator.generate_chunk(&mut world, 0, 0);
        }
        let chunk = chunks.remove(&(0, 0)).unwrap();

        let encoded = blob::encode(&chunk);
        assert!(
            encoded.len() < 16 * 1024,
            "编码后仍然太大: {} 字节",
            encoded.len()
        );

        let back = blob::decode(0, 0, &encoded).unwrap();
        for y in 0..crate::level::format::chunk::CHUNK_HEIGHT {
            for x in 0..16 {
                for z in 0..16 {
                    assert_eq!(back.block(x, y, z), chunk.block(x, y, z), "{x},{y},{z}");
                }
            }
        }
    }
}
