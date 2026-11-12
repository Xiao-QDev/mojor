// 自研存档格式 mojoregion
// region 是 32x32 个区块一个文件 文件内部只追加 索引双槽交替保证崩溃后仍可读
// 区块本体按 section 分类编码 空气段与全同值段几乎不占空间
pub mod blob;
pub mod region_file;
pub mod store;

pub use blob::{decode, encode};
pub use store::RegionStore;

#[cfg(test)]
mod probe {
    use super::*;
    use crate::level::format::chunk::Chunk;
    use crate::level::generator::{Generator, world::GenWorld};
    use std::collections::HashMap;

    // 一个满 region 的读写成本 用真实地形做样本
    #[test]
    #[ignore]
    fn storage_timing() {
        let mut generator = Generator::terrain(1337);
        let mut chunks = HashMap::new();
        for cz in 0..32 {
            for cx in 0..32 {
                chunks.insert((cx, cz), Chunk::empty(cx, cz));
            }
        }
        {
            let mut world = GenWorld::new(&mut chunks);
            for cz in 0..32 {
                for cx in 0..32 {
                    generator.generate_chunk(&mut world, cx, cz);
                }
            }
        }

        let t = std::time::Instant::now();
        let mut blobs: Vec<((i32, i32), Vec<u8>)> = Vec::with_capacity(1024);
        for cz in 0..32 {
            for cx in 0..32 {
                blobs.push(((cx, cz), blob::encode(&chunks[&(cx, cz)])));
            }
        }
        let encode_time = t.elapsed();

        let total: usize = blobs.iter().map(|(_, data)| data.len()).sum();
        let raw = chunks[&(0, 0)].serialize().len();
        println!(
            "encode 1024 chunks: {:?} ({:.1} us/chunk)",
            encode_time,
            encode_time.as_secs_f64() * 1e6 / 1024.0
        );
        println!(
            "encoded size: {} bytes total, {} bytes/chunk (raw {})",
            total,
            total / 1024,
            raw
        );

        let dir = std::env::temp_dir().join(format!(
            "mojor-region-bench-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = RegionStore::open(&dir).unwrap();

        // 裸写基线 同样多的字节直接顺序写一个文件 用来区分格式开销和磁盘开销
        let baseline = dir.join("baseline.bin");
        let t = std::time::Instant::now();
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&baseline).unwrap();
            for (_, data) in &blobs {
                file.write_all(data).unwrap();
            }
            file.flush().unwrap();
        }
        let baseline_write = t.elapsed();
        let t = std::time::Instant::now();
        {
            let file = std::fs::File::open(&baseline).unwrap();
            file.sync_all().unwrap();
        }
        println!(
            "baseline raw write: {:?}, sync: {:?}",
            baseline_write,
            t.elapsed()
        );

        let t = std::time::Instant::now();
        store.save(&blobs).unwrap();
        let save_time = t.elapsed();
        println!(
            "save 1024 chunks: {:?} ({:.1} us/chunk)",
            save_time,
            save_time.as_secs_f64() * 1e6 / 1024.0
        );

        let t = std::time::Instant::now();
        store.sync_all().unwrap();
        println!("sync_all (background): {:?}", t.elapsed());

        // 拆开写数据与刷盘 看 fsync 到底占多少
        let probe_path = dir.join("region").join("r.9.9.mjr");
        let mut probe = region_file::RegionFile::open(&probe_path, 9, 9).unwrap();
        let t = std::time::Instant::now();
        for (i, (_, data)) in blobs.iter().enumerate() {
            let cx = 9 * 32 + (i % 32) as i32;
            let cz = 9 * 32 + (i / 32) as i32;
            probe.write_chunk(cx, cz, data).unwrap();
        }
        let write_only = t.elapsed();
        let t = std::time::Instant::now();
        probe.flush().unwrap();
        let flush_only = t.elapsed();
        let t = std::time::Instant::now();
        probe.sync().unwrap();
        let sync_only = t.elapsed();
        println!(
            "write_chunk x1024: {:?} ({:.1} us/chunk), index flush: {:?}, fsync: {:?}",
            write_only,
            write_only.as_secs_f64() * 1e6 / 1024.0,
            flush_only,
            sync_only
        );

        let t = std::time::Instant::now();
        let mut bytes = 0;
        for ((cx, cz), _) in &blobs {
            bytes += store.load(*cx, *cz).unwrap().unwrap().len();
        }
        let load_time = t.elapsed();
        println!(
            "load 1024 chunks: {:?} ({:.1} us/chunk, {} bytes)",
            load_time,
            load_time.as_secs_f64() * 1e6 / 1024.0,
            bytes
        );

        let t = std::time::Instant::now();
        for ((cx, cz), data) in &blobs {
            let back = blob::decode(*cx, *cz, data).unwrap();
            std::hint::black_box(back);
        }
        println!("decode 1024 chunks: {:?}", t.elapsed());

        store.close().unwrap();
        let size = std::fs::read_dir(dir.join("region"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.metadata().unwrap().len())
            .sum::<u64>();
        println!("region file size: {} bytes", size);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
