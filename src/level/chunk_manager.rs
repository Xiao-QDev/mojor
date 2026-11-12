use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bytes::Bytes;
use tracing::warn;

use crate::error::Result;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::format::chunk::{CHUNK_HEIGHT, Chunk};
use crate::level::format::mojoregion::blob;
use crate::level::generator::Generator;
use crate::level::generator::world::GenWorld;
use crate::level::light;

// 业务 GC 的宽限期 不可达之后留 2 秒 防玩家在区块边界反复横跳导致重复生成
// 原样照搬 Nukkit 的 30 秒会让"走出旧区域"后旧区块驻留太久
const UNLOAD_DELAY_TICKS: u64 = 40;
const MAX_UNLOAD_PER_TICK: usize = 96;

// 区块集合 对齐 Nukkit 的 ChunkManager：内存缓存 + 按需从存档加载 + 脏区块回收
#[derive(Default)]
pub struct ChunkManager {
    chunks: HashMap<(i32, i32), Chunk>,
    // 改动过等待落盘的区块
    dirty: HashSet<(i32, i32)>,
    // 等待卸载的区块 值是入队时的 tick
    unload_queue: HashMap<(i32, i32), u64>,
    ticks: u64,
    loader: Option<Arc<ChunkLoader>>,
    generator: Generator,
    // 本分片负责的区块范围 闭区间 越界的读写一律丢弃
    // 这样同一个区块只会存在于一个 region 里 不会出现两份副本
    bounds: Option<((i32, i32), (i32, i32))>,
}

impl ChunkManager {
    pub fn new() -> Self {
        Self::default()
    }

    // 带存档的构造
    pub fn with_loader(loader: ChunkLoader) -> Self {
        Self::with_shared_loader(Arc::new(loader))
    }

    // 多个 region 共用同一个存档句柄 每个区域文件内部自己加锁
    pub fn with_shared_loader(loader: Arc<ChunkLoader>) -> Self {
        Self {
            loader: Some(loader),
            ..Self::default()
        }
    }

    // 换生成器只影响之后没加载过的区块
    pub fn set_generator(&mut self, generator: Generator) {
        self.generator = generator;
    }

    // region 边界 传 None 表示不限
    pub fn set_bounds(&mut self, bounds: ((i32, i32), (i32, i32))) {
        self.bounds = Some(bounds);
    }

    // 本分片是否负责这个区块 视野下发时用来分流
    pub fn owns(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.in_bounds(chunk_x, chunk_z)
    }

    fn in_bounds(&self, chunk_x: i32, chunk_z: i32) -> bool {
        match self.bounds {
            Some(((min_x, min_z), (max_x, max_z))) => {
                (min_x..=max_x).contains(&chunk_x) && (min_z..=max_z).contains(&chunk_z)
            }
            None => true,
        }
    }

    // 取区块 缺了就按 Nukkit 的两阶段补齐 先生成地形再装饰
    pub fn get(&mut self, chunk_x: i32, chunk_z: i32) -> &mut Chunk {
        let key = (chunk_x, chunk_z);
        if !self.chunks.contains_key(&key) {
            self.insert_loaded(chunk_x, chunk_z);
        }
        self.ensure_generated(chunk_x, chunk_z);
        self.ensure_populated(chunk_x, chunk_z);
        self.chunks.get_mut(&key).expect("区块刚刚插入 不可能不存在")
    }

    // 存档里有就用存档 否则先放一个空区块等生成
    fn insert_loaded(&mut self, chunk_x: i32, chunk_z: i32) {
        let data = match self.loader.as_ref() {
            Some(loader) => match loader.load(chunk_x, chunk_z) {
                Ok(data) => data,
                Err(e) => {
                    warn!(chunk_x, chunk_z, error = %e, "读取区块失败 重新生成");
                    None
                }
            },
            None => None,
        };

        let chunk = data
            .and_then(|data| blob::decode(chunk_x, chunk_z, &data))
            .unwrap_or_else(|| Chunk::empty(chunk_x, chunk_z));
        self.chunks.insert((chunk_x, chunk_z), chunk);
    }

    // 第一阶段：铺基础地形
    fn ensure_generated(&mut self, chunk_x: i32, chunk_z: i32) {
        // 装饰只需要本 region 内的邻居 越界的留给隔壁自己处理
        if !self.in_bounds(chunk_x, chunk_z) {
            return;
        }
        let key = (chunk_x, chunk_z);
        if self.chunks.get(&key).is_some_and(|chunk| chunk.generated) {
            return;
        }
        self.chunks
            .entry(key)
            .or_insert_with(|| Chunk::empty(chunk_x, chunk_z));

        {
            // 拆开字段 让生成器能一边持有区块表一边读自身状态
            let Self {
                chunks,
                generator,
                bounds,
                ..
            } = self;
            let mut world = GenWorld::new(chunks).with_bounds(*bounds);
            generator.generate_chunk(&mut world, chunk_x, chunk_z);
        }

        if let Some(chunk) = self.chunks.get_mut(&key) {
            chunk.generated = true;
        }
    }

    // 第二阶段：装饰 需要周围一圈区块已经铺好地形
    fn ensure_populated(&mut self, chunk_x: i32, chunk_z: i32) {
        let key = (chunk_x, chunk_z);
        if self.chunks.get(&key).is_some_and(|chunk| chunk.populated) {
            return;
        }

        for dx in -1..=1 {
            for dz in -1..=1 {
                self.ensure_generated(chunk_x + dx, chunk_z + dz);
            }
        }

        {
            let Self {
                chunks,
                generator,
                bounds,
                ..
            } = self;
            let mut world = GenWorld::new(chunks).with_bounds(*bounds);
            generator.populate_chunk(&mut world, chunk_x, chunk_z);
        }

        if let Some(chunk) = self.chunks.get_mut(&key) {
            chunk.populated = true;
            light::rebuild(chunk);
        }
    }

    // 区块已经在内存缓存里（已生成或已加载）才返回 true
    // 异步加载改造后 下发路径靠它判断要不要先把区块交给 IO 池
    pub fn is_cached(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks.contains_key(&(chunk_x, chunk_z))
    }

    // IO 池把区块 load+decode 回来之后塞进缓存
    // 磁盘上根本没有（返回 None）就现场生成 与 insert_loaded 的兜底一致
    pub fn insert_loaded_chunk(&mut self, chunk_x: i32, chunk_z: i32, chunk: Option<Chunk>) {
        match chunk {
            Some(chunk) => {
                self.chunks.insert((chunk_x, chunk_z), chunk);
            }
            None => {
                self.chunks
                    .entry((chunk_x, chunk_z))
                    .or_insert_with(|| Chunk::empty(chunk_x, chunk_z));
                self.ensure_generated(chunk_x, chunk_z);
                self.ensure_populated(chunk_x, chunk_z);
            }
        }
    }

    // 区块序列化结果 未修改时走缓存
    pub fn payload(&mut self, chunk_x: i32, chunk_z: i32) -> Bytes {
        self.get(chunk_x, chunk_z).payload()
    }

    // 只读缓存 未命中返回 None 不触发磁盘加载
    // 给同步调用方(global tick)用 避免在非 region 线程上做同步读盘
    pub fn payload_cached(&self, chunk_x: i32, chunk_z: i32) -> Option<Bytes> {
        self.chunks
            .get(&(chunk_x, chunk_z))
            .map(|chunk| chunk.payload())
    }

    pub fn block(&mut self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.get(x >> 4, z >> 4)
            .block((x & 0xf) as usize, y as usize, (z & 0xf) as usize)
    }

    // 写方块 返回值表示是否真的发生了变化
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u8) -> bool {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return false;
        }
        let (chunk_x, chunk_z) = (x >> 4, z >> 4);
        // 属主校验 别的分片的方块不该由本线程改 越界要当场暴露而不是写坏
        debug_assert!(
            self.owns(chunk_x, chunk_z),
            "改到了不属于本分片的区块 ({chunk_x}, {chunk_z})"
        );
        let (lx, ly, lz) = ((x & 0xf) as usize, y as usize, (z & 0xf) as usize);

        let chunk = self.get(chunk_x, chunk_z);
        if chunk.block(lx, ly, lz) == id {
            return false;
        }
        chunk.set_block(lx, ly, lz, id);
        // 光照按列算 改一格只重算这一列 挖开方块后光马上就能漏下去
        light::rebuild_column(chunk, lx, lz);
        self.dirty.insert((chunk_x, chunk_z));
        true
    }

    // region 自己的 tick 目前只做区块回收 以后的生物 AI 流体 红石也挂这里
    // in_use 是玩家当前视野覆盖的区块 由 region 线程算好后传进来
    // 卸载只在本线程发生 符合 Folia 的"只由属主 region 卸载"
    pub fn tick(&mut self, in_use: &HashSet<(i32, i32)>) -> usize {
        self.ticks += 1;
        self.run_unload(in_use, false)
    }

    // 测试跳过 30 秒等待 与 Nukkit 的 unloadChunks(force) 对应
    #[cfg(test)]
    pub fn tick_force(&mut self, in_use: &HashSet<(i32, i32)>) -> usize {
        self.ticks += 1;
        self.run_unload(in_use, true)
    }

    fn run_unload(&mut self, in_use: &HashSet<(i32, i32)>, force: bool) -> usize {
        // 不在用的入队 重新进入视野就取消 出生点周围 3x3 常驻
        for &key in self.chunks.keys() {
            if in_use.contains(&key) || Self::is_spawn_chunk(key.0, key.1) {
                self.unload_queue.remove(&key);
            } else {
                self.unload_queue.entry(key).or_insert(self.ticks);
            }
        }
        self.unload_queue.retain(|key, _| self.chunks.contains_key(key));

        let mut expired = Vec::new();
        for (&key, &queued_at) in &self.unload_queue {
            if expired.len() >= MAX_UNLOAD_PER_TICK {
                break;
            }
            if !force && self.ticks.saturating_sub(queued_at) < UNLOAD_DELAY_TICKS {
                continue;
            }
            if in_use.contains(&key) || Self::is_spawn_chunk(key.0, key.1) {
                continue;
            }
            expired.push(key);
        }

        // 脏块先落盘 写失败就留到下一轮 不能把没保存的改动从内存丢掉
        let dirty: Vec<(i32, i32)> = expired
            .iter()
            .filter(|key| self.dirty.contains(key))
            .copied()
            .collect();
        let mut failed = HashSet::new();
        if !dirty.is_empty() {
            let batch: Vec<((i32, i32), Vec<u8>)> = dirty
                .iter()
                .filter_map(|key| {
                    self.chunks
                        .get(key)
                        .map(|chunk| (*key, blob::encode(chunk)))
                })
                .collect();
            match &self.loader {
                Some(loader) => {
                    if loader.save(&batch).is_err() {
                        failed.extend(dirty.iter().copied());
                    }
                }
                None => {}
            }
            for key in &dirty {
                if !failed.contains(key) {
                    self.dirty.remove(key);
                }
            }
        }

        let mut removed = 0;
        for key in expired {
            if failed.contains(&key) {
                // 重新入队交给下个 tick 重试
                self.unload_queue.insert(key, self.ticks);
                continue;
            }
            self.unload_queue.remove(&key);
            self.chunks.remove(&key);
            removed += 1;
        }
        removed
    }

    // 与 Nukkit isSpawnChunk 一致 出生区块周围一格不卸载
    fn is_spawn_chunk(chunk_x: i32, chunk_z: i32) -> bool {
        let spawn_x = crate::network::SPAWN_BLOCK.0 >> 4;
        let spawn_z = crate::network::SPAWN_BLOCK.2 >> 4;
        (chunk_x - spawn_x).abs() <= 1 && (chunk_z - spawn_z).abs() <= 1
    }

    // 挖掉一格 返回原方块 id 空气返回 None
    // 读和写在同一个消息里完成 外部不用先查再改
    pub fn break_block(&mut self, x: i32, y: i32, z: i32) -> Option<u8> {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return None;
        }
        let key = (x >> 4, z >> 4);
        // 属主校验 挖方块也只能动自己分片里的区块
        debug_assert!(
            self.owns(key.0, key.1),
            "挖到了不属于本分片的区块 ({}, {})",
            key.0,
            key.1
        );
        let (lx, lz) = ((x & 0xf) as usize, (z & 0xf) as usize);
        let id = {
            let chunk = self.get(key.0, key.1);
            let id = chunk.block(lx, y as usize, lz);
            if id == 0 {
                return None;
            }
            chunk.set_block(lx, y as usize, lz, 0);
            light::rebuild_column(chunk, lx, lz);
            id
        };
        self.dirty.insert(key);
        Some(id)
    }

    // 该列最高的非空气方块 没有则返回 0
    // 高度图在装饰与每次改方块时都会重算 直接读它就是 O(1)
    pub fn highest_block(&mut self, x: i32, z: i32) -> i32 {
        let chunk = self.get(x >> 4, z >> 4);
        chunk.height((x & 0xf) as usize, (z & 0xf) as usize) as i32
    }

    pub fn highest_blocks(&mut self, columns: &[(i32, i32)]) -> Vec<i32> {
        columns
            .iter()
            .map(|&(x, z)| self.highest_block(x, z))
            .collect()
    }

    // 批量读方块 一次把整批要用的坐标都取出来
    pub fn blocks(&mut self, probes: &[(i32, i32, i32)]) -> Vec<u8> {
        probes
            .iter()
            .map(|&(x, y, z)| self.block(x, y, z))
            .collect()
    }

    // 批量取区块序列化结果
    pub fn payloads(&mut self, chunks: &[(i32, i32)]) -> Vec<Bytes> {
        chunks
            .iter()
            .map(|&(chunk_x, chunk_z)| self.payload(chunk_x, chunk_z))
            .collect()
    }

    // 分批落盘 每 tick 只编码并写出少量脏块
    // 大批量改动时不会被一次性编码几十 MB 卡住 region tick
    pub fn flush_limited(&mut self, limit: usize) -> Result<usize> {
        if self.dirty.is_empty() || limit == 0 {
            return Ok(0);
        }
        let keys: Vec<(i32, i32)> = self.dirty.iter().take(limit).copied().collect();
        let batch: Vec<((i32, i32), Vec<u8>)> = keys
            .iter()
            .filter_map(|key| {
                self.chunks
                    .get(key)
                    .map(|chunk| (*key, blob::encode(chunk)))
            })
            .collect();

        {
            let Some(loader) = &self.loader else {
                return Ok(0);
            };
            loader.save(&batch)?;
        }

        for (key, _) in &batch {
            self.dirty.remove(key);
        }
        Ok(batch.len())
    }

    // 把脏区块写回存档 返回写入的区块数
    pub fn flush(&mut self) -> Result<usize> {
        if self.dirty.is_empty() {
            return Ok(0);
        }

        let batch: Vec<((i32, i32), Vec<u8>)> = self
            .dirty
            .iter()
            .filter_map(|key| {
                self.chunks
                    .get(key)
                    .map(|chunk| (*key, blob::encode(chunk)))
            })
            .collect();

        {
            let Some(loader) = &self.loader else {
                return Ok(0);
            };
            loader.save(&batch)?;
        }

        // 写成功后才清脏标记 失败时留给下个周期重试
        for (key, _) in &batch {
            self.dirty.remove(key);
        }
        Ok(batch.len())
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::flat::{self, GRASS};

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mojor-{name}-{}-{}.redb",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn generates_flat_chunk_on_demand() {
        let mut manager = ChunkManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.block(0, 0, 0), flat::BEDROCK);
        assert_eq!(manager.block(0, 4, 0), GRASS);
        // 装饰阶段要连带生成周围一圈区块
        assert_eq!(manager.len(), 9);
    }

    // 负坐标要落到正确的区块
    #[test]
    fn negative_coords_map_to_correct_chunk() {
        let mut manager = ChunkManager::new();
        assert!(manager.set_block(-1, 5, -1, 1));
        assert_eq!(manager.block(-1, 5, -1), 1);
        assert_eq!(manager.len(), 9);
        // 原方块仍在
        assert_eq!(manager.block(0, 5, 0), 0);
    }

    // 挖掉地表的方块 下一层必须立刻见到天光
    #[test]
    fn breaking_block_updates_light_immediately() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        assert_eq!(manager.get(0, 0).sky_light(0, 3, 0), 0);

        assert!(manager.set_block(0, 4, 0, 0));

        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 3);
        assert_eq!(chunk.sky_light(0, 3, 0), 15);
        assert_eq!(chunk.sky_light(0, 2, 0), 0);
    }

    // 在空地上放方块 下方立刻变暗
    #[test]
    fn placing_block_darkens_below() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.get(0, 0).sky_light(0, 20, 0), 15);

        assert!(manager.set_block(0, 10, 0, 1));

        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 10);
        assert_eq!(chunk.sky_light(0, 10, 0), 15);
        assert_eq!(chunk.sky_light(0, 9, 0), 0);
        assert_eq!(chunk.sky_light(0, 11, 0), 15);
    }

    #[test]
    fn set_block_reports_change() {
        let mut manager = ChunkManager::new();
        assert!(manager.set_block(0, 5, 0, 1));
        assert!(!manager.set_block(0, 5, 0, 1));
        // 超出高度直接忽略
        assert!(!manager.set_block(0, 200, 0, 1));
        assert_eq!(manager.block(0, 200, 0), 0);
    }

    // 修改后序列化缓存要失效
    #[test]
    fn payload_reflects_edits() {
        let mut manager = ChunkManager::new();
        let before = manager.payload(0, 0);
        assert_eq!(before[5], 0);
        manager.set_block(0, 5, 0, 3);
        let after = manager.payload(0, 0);
        assert_eq!(after[5], 3);
    }

    // 装饰结束后要重算高度图与天空光照
    #[test]
    fn light_and_height_after_decoration() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 4);
        // 露出天空的部分全亮
        assert_eq!(chunk.sky_light(0, 5, 0), 15);
        assert_eq!(chunk.sky_light(0, CHUNK_HEIGHT - 1, 0), 15);
        // 地表方块自身受光 它下面才是暗的
        assert_eq!(chunk.sky_light(0, 4, 0), 15);
        assert_eq!(chunk.sky_light(0, 3, 0), 0);
        assert_eq!(chunk.sky_light(0, 0, 0), 0);
    }

    // 改过的方块落盘后重新打开还在 没改的仍是平坦地形
    #[test]
    fn flush_persists_edits() {
        let path = temp_path("store-persist");
        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            assert!(manager.set_block(0, 5, 0, 7));
            assert_eq!(manager.flush().unwrap(), 1);
            // 再 flush 一次没有新改动
            assert_eq!(manager.flush().unwrap(), 0);
        }

        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            assert_eq!(manager.block(0, 5, 0), 7);
            // 未修改过的区块仍然现生成
            assert_eq!(manager.block(0, 4, 0), GRASS);
            assert!(manager.flush().unwrap() == 0);
        }

        let _ = std::fs::remove_file(&path);
    }

    // 没有存档时 flush 什么都不做
    #[test]
    fn flush_without_loader_is_noop() {
        let mut manager = ChunkManager::new();
        manager.set_block(0, 5, 0, 1);
        assert_eq!(manager.flush().unwrap(), 0);
    }

    // 与 Nukkit 一致 不在用的区块先入队 30 秒后才真正卸载
    #[test]
    fn unused_chunks_are_unloaded_after_delay() {
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::flat());
        manager.payload(5, 5);
        let before = manager.len();
        assert!(before > 0);

        let idle = HashSet::new();
        // 第一个 tick 才入队 所以要再跑满 600 个 tick 才到期
        for _ in 0..UNLOAD_DELAY_TICKS {
            assert_eq!(manager.tick(&idle), 0);
        }
        assert_eq!(manager.len(), before, "没到 30 秒就卸载了");

        assert_eq!(manager.tick(&idle), before);
        assert_eq!(manager.len(), 0);
    }

    // 玩家视野里的区块不受卸载影响
    #[test]
    fn chunks_in_use_are_kept() {
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::flat());
        manager.payload(5, 5);
        let before = manager.len();

        let mut in_use = HashSet::new();
        in_use.insert((5, 5));
        let removed = manager.tick_force(&in_use);
        assert!(removed > 0, "视野外的区块没被卸载");
        assert!(manager.chunks.contains_key(&(5, 5)));
        assert_eq!(manager.len(), before - removed);
    }

    // 出生区块周围 3x3 常驻 与 Nukkit isSpawnChunk 一致
    #[test]
    fn spawn_chunks_are_kept() {
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::flat());
        manager.payload(0, 0);
        let before = manager.len();
        assert_eq!(manager.tick_force(&HashSet::new()), 0);
        assert_eq!(manager.len(), before);
    }

    // 卸载前脏块要落盘 卸载后重新加载改动还在
    #[test]
    fn dirty_chunk_is_saved_before_unload() {
        let path = temp_path("unload-dirty");
        let (x, z) = (5 * 16 + 1, 5 * 16 + 1);
        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            manager.set_generator(Generator::flat());
            assert!(manager.set_block(x, 5, z, 9));
            assert!(manager.len() > 0);
            assert!(manager.tick_force(&HashSet::new()) > 0);
            assert_eq!(manager.len(), 0);
        }

        let mut reloaded = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
        assert_eq!(reloaded.block(x, 5, z), 9);
        let _ = std::fs::remove_file(&path);
    }
}


