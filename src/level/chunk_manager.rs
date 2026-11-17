use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use bytes::Bytes;

use crate::error::Result;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::format::chunk::{CHUNK_HEIGHT, Chunk};
use crate::level::format::violinregion::blob;
use crate::level::generator::blocks::is_breakable;
use crate::level::generator::Generator;
use crate::level::generator::world::GenWorld;
use crate::level::actor::RegionMsg;
use crate::level::light::{BlockChange, LightJob, LightKind, LightReceipt, Snapshot};

// 业务 GC 的宽限期 不可达之后留 2 秒 防玩家在区块边界反复横跳导致重复生成
// 原样照搬 Nukkit 的 30 秒会让"走出旧区域"后旧区块驻留太久
const UNLOAD_DELAY_TICKS: u64 = 40;
const MAX_UNLOAD_PER_TICK: usize = 96;
// 纯地形缓存 装饰目标连续时 7x7 的窗口每走一格只换掉一条边
const TERRAIN_CACHE: usize = 256;
// 装饰结果缓存 工作量上限就是一个起源区块只模拟一次
const DECORATION_CACHE: usize = 256;

// 装饰在一个区块上写下的一个方块（全局坐标）
struct BlockWrite {
    x: i32,
    y: i32,
    z: i32,
    id: u8,
    meta: u8,
}

// 一个起源区块的装饰差异。writes 按目标区块排好序，chunks 记录每个目标
// 区块对应的下标区间，合并时只取落在自己身上的那一段。
struct Decoration {
    writes: Vec<BlockWrite>,
    chunks: HashMap<(i32, i32), (usize, usize)>,
}

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
    // 纯地形缓存（未装饰）。population 的只读基层，也是正式区块的地形来源；
    // 不受 region 边界限制，边界外的地形只是装饰上下文，不进世界。
    terrain: HashMap<(i32, i32), Chunk>,
    terrain_order: VecDeque<(i32, i32)>,
    // 每个起源区块的装饰差异。同一区块只模拟一次，之后平移到任何目标区块复用。
    decorations: HashMap<(i32, i32), Arc<Decoration>>,
    decoration_order: VecDeque<(i32, i32)>,
    // 等光照引擎全量计算的新区块 生成后 lit=false 算完回执才 lit=true
    pending_light_init: HashSet<(i32, i32)>,
    // Init 已经提交给 worker、还没回执的区块。回执只对这些区块开 lit：
    // 快照是 3x3 的，邻居区块在回执里只有边界几格是新算的，拿它当全量
    // 会把整块未照亮的地皮下发给客户端
    light_init_in_flight: HashSet<(i32, i32)>,
    // 玩家改动批量收集 攒到 tick 交给光照引擎做增量修正
    // 已 lit 的记；Init 在途的也记，快照抓早了，回执回来后要再补一次 Update
    light_changes: HashMap<(i32, i32), Vec<BlockChange>>,
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

    // 换生成器只影响之后没加载过的区块 地形与装饰缓存全部作废
    pub fn set_generator(&mut self, generator: Generator) {
        self.generator = generator;
        self.terrain.clear();
        self.terrain_order.clear();
        self.decorations.clear();
        self.decoration_order.clear();
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
            Some(loader) => loader.load(chunk_x, chunk_z).unwrap_or_else(|_| {
                None
            }),
            None => None,
        };

        let chunk = data
            .and_then(|data| blob::decode(chunk_x, chunk_z, &data))
            .unwrap_or_else(|| Chunk::empty(chunk_x, chunk_z));
        let key = (chunk_x, chunk_z);
        self.chunks.insert(key, chunk);
        // 存档解码出来的区块 lit 固定为 false。generated/populated 已经是 true，
        // ensure_generated/ensure_populated 不会再补光照任务；这里必须显式排队，
        // 否则区块会一直卡在下发门槛外，表现为走过的地方再也刷不出地形。
        self.pending_light_init.insert(key);
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
        // 装饰阶段已经算过这个区块的地形，直接克隆，不再重复生成
        if let Some(terrain) = self.terrain.get(&key) {
            let mut chunk = terrain.clone();
            chunk.generated = true;
            self.chunks.insert(key, chunk);
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

    // 取纯地形（未装饰）。与正式区块不同，不受 region 边界限制：
    // 边界另一侧的地形只是装饰上下文，用来判断树冠或建筑能不能伸过来。
    fn ensure_terrain(&mut self, chunk_x: i32, chunk_z: i32) {
        let key = (chunk_x, chunk_z);
        if self.terrain.contains_key(&key) {
            return;
        }
        self.terrain.insert(key, Chunk::empty(chunk_x, chunk_z));
        {
            // 拆开字段 生成时只需要地形表和生成器
            let Self {
                terrain, generator, ..
            } = self;
            let mut world = GenWorld::new(terrain);
            generator.generate_chunk(&mut world, chunk_x, chunk_z);
        }
        self.terrain_order.push_back(key);
        while self.terrain.len() > TERRAIN_CACHE {
            if let Some(oldest) = self.terrain_order.pop_front() {
                self.terrain.remove(&oldest);
            }
        }
    }

    // 一个起源区块的装饰差异（相对纯地形的最终状态）。
    // 每个起源区块只在固定的 ±2 地形上下文里模拟一次：同一区块无论从哪个
    // 目标窗口看过来，写入都完全一样，所以缓存可以跨目标复用。
    fn decoration_writes(&mut self, chunk_x: i32, chunk_z: i32) -> Arc<Decoration> {
        let key = (chunk_x, chunk_z);
        if let Some(cached) = self.decorations.get(&key) {
            return cached.clone();
        }

        let mut working = HashMap::new();
        {
            let Self {
                generator, terrain, ..
            } = self;
            let bounds = Some(((chunk_x - 2, chunk_z - 2), (chunk_x + 2, chunk_z + 2)));
            let mut world = GenWorld::with_base(&mut working, terrain).with_bounds(bounds);
            generator.populate_chunk(&mut world, chunk_x, chunk_z);
        }

        // 只记最终状态与地形的差异。同一个方块被同一棵树多次写过也只留一条。
        // 装饰只改地表附近，绝大部分 16 层高的段完全一样，所以按段整块比较，
        // 只有段内有差异时才逐格展开。
        let mut writes = Vec::new();
        for (source, decorated) in working {
            let Some(original) = self.terrain.get(&source) else {
                continue;
            };
            let (ids, meta, ..) = decorated.parts();
            let (base_ids, base_meta, ..) = original.parts();
            for z in 0..16usize {
                for x in 0..16usize {
                    let column = (x << 11) | (z << 7);
                    for offset in (0..CHUNK_HEIGHT).step_by(16) {
                        let nibble = (x << 10) | (z << 6) | (offset >> 1);
                        if ids[column + offset..column + offset + 16]
                            == base_ids[column + offset..column + offset + 16]
                            && meta[nibble..nibble + 8] == base_meta[nibble..nibble + 8]
                        {
                            continue;
                        }
                        for y in offset..offset + 16 {
                            let block = decorated.block(x, y, z);
                            let block_meta = decorated.meta(x, y, z);
                            if block != original.block(x, y, z)
                                || block_meta != original.meta(x, y, z)
                            {
                                writes.push(BlockWrite {
                                    x: source.0 * 16 + x as i32,
                                    y: y as i32,
                                    z: source.1 * 16 + z as i32,
                                    id: block,
                                    meta: block_meta,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 按目标区块排序，合并时按区间取，避免每个目标都扫整份写入
        writes.sort_unstable_by_key(|write| (write.x >> 4, write.z >> 4));
        let mut chunks: HashMap<(i32, i32), (usize, usize)> = HashMap::new();
        for (index, write) in writes.iter().enumerate() {
            let entry = chunks
                .entry((write.x >> 4, write.z >> 4))
                .or_insert((index, index));
            entry.1 = index + 1;
        }

        let writes = Arc::new(Decoration { writes, chunks });
        self.decorations.insert(key, writes.clone());
        self.decoration_order.push_back(key);
        while self.decorations.len() > DECORATION_CACHE {
            if let Some(oldest) = self.decoration_order.pop_front() {
                self.decorations.remove(&oldest);
            }
        }
        writes
    }

    // 第二阶段：装饰 需要周围一圈区块已经铺好地形
    fn ensure_populated(&mut self, chunk_x: i32, chunk_z: i32) {
        let key = (chunk_x, chunk_z);
        if self.chunks.get(&key).is_some_and(|chunk| chunk.populated) {
            return;
        }

        // RTG 的每个起源区块按自己的 ±2 上下文模拟；装饰落点从区块中心 +8 开始，
        // 树冠最远再伸出十几格，写入最远能落到起源的 +2 区块。目标区块合并
        // origin-2..origin+1 的 4x4 起源，每个起源要 ±2 的地形上下文，
        // 合起来正好是目标 -4..+3，不用多准备一圈。
        if matches!(self.generator, Generator::Realistic(_)) {
            for dx in -4..=3 {
                for dz in -4..=3 {
                    self.ensure_terrain(chunk_x + dx, chunk_z + dz);
                }
            }
        } else {
            for dx in -2..=2 {
                for dz in -2..=2 {
                    self.ensure_terrain(chunk_x + dx, chunk_z + dz);
                }
            }
        }

        for dx in -1..=1 {
            for dz in -1..=1 {
                self.ensure_generated(chunk_x + dx, chunk_z + dz);
            }
        }

        let completed = {
            let mut chunk = if matches!(self.generator, Generator::Realistic(_)) {
                // 树和村庄会跨区块写入。每个起源区块只装饰一次并缓存差异，
                // 目标区块按 dx/dz 顺序合并 16 份落在自己身上的写入；同一区块
                // 无论从哪个窗口加载，结果都完全一致，跨界的树不会被砍半。
                let mut center = self
                    .terrain
                    .get(&key)
                    .expect("目标区块的地形已经准备好")
                    .clone();
                for dx in -2..=1 {
                    for dz in -2..=1 {
                        let decoration = self.decoration_writes(chunk_x + dx, chunk_z + dz);
                        let Some(&(start, end)) = decoration.chunks.get(&key) else {
                            continue;
                        };
                        for write in &decoration.writes[start..end] {
                            let (lx, lz) = ((write.x & 15) as usize, (write.z & 15) as usize);
                            center.set_block(lx, write.y as usize, lz, write.id);
                            center.set_meta(lx, write.y as usize, lz, write.meta);
                        }
                    }
                }
                center
            } else {
                // 其他生成器只写自己的区块，共享视图里直接装饰即可。
                let mut working = HashMap::new();
                {
                    let Self {
                        generator, terrain, ..
                    } = self;
                    let bounds = Some(((chunk_x - 2, chunk_z - 2), (chunk_x + 2, chunk_z + 2)));
                    let mut world = GenWorld::with_base(&mut working, terrain).with_bounds(bounds);
                    for dx in -1..=1 {
                        for dz in -1..=1 {
                            generator.populate_chunk(&mut world, chunk_x + dx, chunk_z + dz);
                        }
                    }
                }
                working
                    .remove(&key)
                    .unwrap_or_else(|| self.terrain[&key].clone())
            };

            chunk.generated = true;
            chunk.populated = true;
            // 装饰写入不走 ChunkManager::set_block 高度图在这里统一刷
            // 出生点与碰撞查询直接读它 不能等异步光照
            chunk.refresh_heights();
            // 光照交给异步引擎 lit=false 的区块不下发也不落盘
            chunk.lit = false;
            chunk
        };

        self.chunks.insert(key, completed);
        self.pending_light_init.insert(key);
        // 装饰完成才算"完整区块" 标脏交给 unload/flush 落盘
        // 只标本分片内的区块 越界的由属主 region 自己负责
        if self.in_bounds(chunk_x, chunk_z) {
            self.dirty.insert(key);
        }
    }

    // 区块已经在内存缓存里（已生成或已加载）才返回 true
    // 异步加载改造后 下发路径靠它判断要不要先把区块交给 IO 池
    // 已加载区块数 健康报告用
    pub fn loaded_chunks(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_cached(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks.contains_key(&(chunk_x, chunk_z))
    }

    // 装饰阶段会顺手把邻居铺成"只有地形"的半成品 它们占着缓存但没有光照任务
    // 下发路径撞上这种区块要补一次完整流程 否则它会一直 card 在等光照
    pub fn prepare_for_light(&mut self, chunk_x: i32, chunk_z: i32) {
        self.ensure_generated(chunk_x, chunk_z);
        self.ensure_populated(chunk_x, chunk_z);
    }

    // IO 池把区块 load+decode 回来之后塞进缓存
    // 磁盘上根本没有（返回 None）就现场生成 与 insert_loaded 的兜底一致
    pub fn insert_loaded_chunk(&mut self, chunk_x: i32, chunk_z: i32, chunk: Option<Chunk>) {
        match chunk {
            Some(chunk) => {
                let key = (chunk_x, chunk_z);
                self.chunks.insert(key, chunk);
                // 与 insert_loaded 同理：磁盘上的区块带光照数据但 lit=false，
                // 不补一次 Init 就永远等不到下发。
                self.pending_light_init.insert(key);
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
    // 光照没算完(lit=false)也回 None 复用同一套重试 下轮再来
    pub fn payload_cached(&self, chunk_x: i32, chunk_z: i32) -> Option<Bytes> {
        self.chunks
            .get(&(chunk_x, chunk_z))
            .filter(|chunk| chunk.lit)
            .map(|chunk| chunk.payload())
    }

    // 只读窥探 光照快照抓取用 不触发生成
    pub fn peek_chunk(&self, chunk_x: i32, chunk_z: i32) -> Option<&Chunk> {
        self.chunks.get(&(chunk_x, chunk_z))
    }

    pub fn block(&mut self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.get(x >> 4, z >> 4)
            .block((x & 0xf) as usize, y as usize, (z & 0xf) as usize)
    }

    // 读方块元数据 越界回 0
    pub fn meta(&mut self, x: i32, y: i32, z: i32) -> u8 {
        if !(0..CHUNK_HEIGHT as i32).contains(&y) {
            return 0;
        }
        self.get(x >> 4, z >> 4)
            .meta((x & 0xf) as usize, y as usize, (z & 0xf) as usize)
    }

    // 写方块 返回值表示是否真的发生了变化
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: u8) -> bool {
        self.set_block_meta(x, y, z, id, 0)
    }

    // 写方块带元数据 玩家放置走这条 羊毛色 木头种类 楼梯朝向 都在 meta 里
    pub fn set_block_meta(&mut self, x: i32, y: i32, z: i32, id: u8, meta: u8) -> bool {
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

        let old = {
            let chunk = self.get(chunk_x, chunk_z);
            let old = chunk.block(lx, ly, lz);
            if old == id && chunk.meta(lx, ly, lz) == meta {
                return false;
            }
            chunk.set_block(lx, ly, lz, id);
            chunk.set_meta(lx, ly, lz, meta);
            // 高度图同步刷新 碰撞与出生点查询不能等异步光照
            chunk.refresh_height(lx, lz);
            old
        };
        // 已 lit 的区块记增量修正交给光照引擎
        // Init 已经在途的也要记：快照在提交时抓好了 这次改动不在里面
        // 等 Init 回执写回之后必须再补一轮 Update 否则光照是旧的
        if self.chunks.get(&(chunk_x, chunk_z)).is_some_and(|chunk| chunk.lit)
            || self.light_init_in_flight.contains(&(chunk_x, chunk_z))
        {
            self.light_changes
                .entry((chunk_x, chunk_z))
                .or_default()
                .push(BlockChange {
                    x,
                    y,
                    z,
                    old_id: old,
                    new_id: id,
                });
        }
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
            // 光照没算完的不卸载 全 0 光照写进存档就是污染
            // 等 LightDone 回填 lit=true 之后走正常卸载
            if !self.chunks.get(&key).is_some_and(|chunk| chunk.lit) {
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
            // 空气没什么可挖 基岩挖不动
            if id == 0 || !is_breakable(id) {
                return None;
            }
            chunk.set_block(lx, y as usize, lz, 0);
            // 元数据一起清 剩下的 meta 会串到以后放上去的方块
            chunk.set_meta(lx, y as usize, lz, 0);
            chunk.refresh_height(lx, lz);
            id
        };
        // 已 lit 或 Init 在途的区块记增量修正交给光照引擎
        // 在途的那份快照抓在改动之前 回执回来后要再补一次
        if self.chunks.get(&key).is_some_and(|chunk| chunk.lit)
            || self.light_init_in_flight.contains(&key)
        {
            self.light_changes.entry(key).or_default().push(BlockChange {
                x,
                y,
                z,
                old_id: id,
                new_id: 0,
            });
        }
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
        // lit=false 的区块跳过编码 留在 dirty 里等光照回执
        let batch: Vec<((i32, i32), Vec<u8>)> = keys
            .iter()
            .filter_map(|key| {
                self.chunks
                    .get(key)
                    .filter(|chunk| chunk.lit)
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

        // lit=false 的区块跳过编码 留在 dirty 里等光照回执
        let batch: Vec<((i32, i32), Vec<u8>)> = self
            .dirty
            .iter()
            .filter_map(|key| {
                self.chunks
                    .get(key)
                    .filter(|chunk| chunk.lit)
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

    // 取走待处理的光照任务提交给异步引擎
    // 快照抓取必须在 region 线程做（ChunkManager 是 region 私有的） 整体 move 给 worker
    pub(crate) fn take_light_jobs(&mut self, reply: &Sender<RegionMsg>) -> Vec<LightJob> {
        let mut jobs = Vec::new();
        let inits: Vec<(i32, i32)> = self.pending_light_init.drain().collect();
        for key in inits {
            // lit=false 的区块不会进卸载队列 这里只是防御
            if !self.chunks.contains_key(&key) {
                continue;
            }
            let snap = Snapshot::grab(self, key.0, key.1);
            self.light_init_in_flight.insert(key);
            jobs.push(LightJob {
                kind: LightKind::Init(snap),
                reply: reply.clone(),
            });
        }
        let updates: Vec<((i32, i32), Vec<BlockChange>)> = self.light_changes.drain().collect();
        for (key, changes) in updates {
            if !self.chunks.contains_key(&key) {
                continue;
            }
            // Init 还在途时快照里的光照还是旧的 现在排增量会拿错底
            // 先放回去 等回执写回之后的下一轮再走
            if self.light_init_in_flight.contains(&key) {
                self.light_changes.insert(key, changes);
                continue;
            }
            let snap = Snapshot::grab(self, key.0, key.1);
            jobs.push(LightJob {
                kind: LightKind::Update { snap, changes },
                reply: reply.clone(),
            });
        }
        jobs
    }

    // 光照回执写回 只合入还在内存里的区块 已卸载的直接丢弃
    // 只有任务的目标区块能开 lit：回执会捎带邻居区块的边界修正，
    // 那几格光不能当整块算完，否则未照亮的地皮会直接下发给客户端
    pub fn apply_light_receipt(&mut self, receipt: LightReceipt) {
        let owner = receipt.owner;
        // Init 回执在光照没有变化时不含目标区块（不变脏就不打包）
        // 但任务本身已经完成 门槛照样要开 不然这个区块永远不下发
        let init_done = self.light_init_in_flight.remove(&owner);
        if init_done {
            if let Some(chunk) = self.chunks.get_mut(&owner) {
                chunk.lit = true;
            }
        }
        for data in receipt.chunks {
            let key = (data.x, data.z);
            if let Some(chunk) = self.chunks.get_mut(&key) {
                let (_, _, sky, light) = chunk.parts_mut();
                sky.copy_from_slice(&data.sky_light);
                light.copy_from_slice(&data.block_light);
                self.dirty.insert(key);
            }
        }
    }

    // 还有光照没落地：待提交的、在途的、或者改了还没送出去的
    // 空闲判定必须带上它，否则分片会在回执到达前被撤掉，
    // 未 lit 的区块被 flush 跳过，玩家的编辑就留在内存里一起没了
    pub fn has_pending_light(&self) -> bool {
        !self.pending_light_init.is_empty()
            || !self.light_changes.is_empty()
            || !self.light_init_in_flight.is_empty()
    }

    // 下发门槛 光照回填之前区块不下发
    pub fn is_lit(&self, chunk_x: i32, chunk_z: i32) -> bool {
        self.chunks
            .get(&(chunk_x, chunk_z))
            .is_some_and(|chunk| chunk.lit)
    }

    // 测试跳过异步光照 直接把内存里的区块全标成已照亮
    #[cfg(test)]
    pub fn mark_all_lit_for_test(&mut self) {
        for chunk in self.chunks.values_mut() {
            chunk.lit = true;
        }
        self.pending_light_init.clear();
        self.light_init_in_flight.clear();
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
    use crate::level::generator::realistic_terrain::structure::map_generator::VillageMapGenerator;
    use crate::level::generator::realistic_terrain::structure::placement::{
        StructurePlacement, VILLAGE_DISTANCE,
    };
    use crate::level::generator::realistic_terrain::structure::topology::PieceKind;

    fn normalize_flower_kinds(chunk: &mut Chunk) {
        // 参考实现的花种使用系统随机源；位置和其他方块仍必须逐格一致。
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..CHUNK_HEIGHT {
                    if matches!(chunk.block(x, y, z), 37 | 38) {
                        chunk.set_block(x, y, z, 38);
                        chunk.set_meta(x, y, z, 0);
                    }
                }
            }
        }
    }

    /// 找一个确有非道路建筑跨区块的 RTG 村庄，返回目标区块和建筑 footprint 内的探针。
    fn cross_chunk_village_fixture() -> (u32, (i32, i32), (i32, i32)) {
        const SEED: u32 = 20261115;
        let placement = StructurePlacement::new(SEED as i64, VILLAGE_DISTANCE);
        let candidates = placement.candidates_in_window(-128, -128, 128, 128);
        let mut villages = VillageMapGenerator::new(SEED as i64);
        for (chunk_x, chunk_z) in candidates {
            let Some(start) = villages.start_at(chunk_x, chunk_z) else {
                continue;
            };
            for piece in &start.pieces {
                if piece.kind == PieceKind::Path {
                    continue;
                }
                let b = piece.bounding_box;
                let first = (b.min_x >> 4, b.min_z >> 4);
                let second = (b.max_x >> 4, b.max_z >> 4);
                if first == second {
                    continue;
                }
                let origin = (start.chunk_x, start.chunk_z);
                let target = if first != origin { first } else { second };
                let min_x = target.0 * 16;
                let min_z = target.1 * 16;
                let probe = (
                    ((b.min_x + b.max_x) / 2).clamp(min_x, min_x + 15),
                    ((b.min_z + b.max_z) / 2).clamp(min_z, min_z + 15),
                );
                return (SEED, target, probe);
            }
        }
        panic!("测试种子没有找到跨区块村庄建筑");
    }

    fn realistic_manager(seed: u32) -> ChunkManager {
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::realistic(seed));
        manager
    }

    fn normalized_chunk(manager: &mut ChunkManager, target: (i32, i32)) -> Bytes {
        let chunk = manager.get(target.0, target.1);
        normalize_flower_kinds(chunk);
        chunk.serialize()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "violin-{name}-{}-{}.redb",
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

    // 挖掉地表的方块 高度图立刻刷新 光照修正排队交给异步引擎
    #[test]
    fn breaking_block_updates_height_and_queues_light() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        manager.mark_all_lit_for_test();
        assert_eq!(manager.get(0, 0).sky_light(0, 3, 0), 0);

        assert!(manager.set_block(0, 4, 0, 0));

        // 高度图同步刷新 出生点与碰撞查询不等异步光照
        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 3);
        // 光照保持旧值 直到 LightDone 回执
        assert_eq!(chunk.sky_light(0, 3, 0), 0);
        // 改动已排队
        assert_eq!(manager.light_changes.get(&(0, 0)).map(Vec::len), Some(1));
    }

    // 在空地上放方块 高度图立刻变 光照修正排队交给异步引擎
    #[test]
    fn placing_block_queues_light_update() {
        let mut manager = ChunkManager::new();
        // 先生成再标 lit set_block 只对已 lit 的区块记增量
        let _ = manager.get(0, 0);
        manager.mark_all_lit_for_test();

        assert!(manager.set_block(0, 10, 0, 1));

        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 10);
        // 光照保持旧值 改动已排队
        assert_eq!(manager.light_changes.get(&(0, 0)).map(Vec::len), Some(1));
    }

    // 基岩挖不动 返回 None 表示什么都没发生
    #[test]
    fn bedrock_cannot_be_broken() {
        let mut manager = ChunkManager::new();
        assert!(manager.set_block(0, 1, 0, crate::level::generator::blocks::BEDROCK));
        assert_eq!(manager.break_block(0, 1, 0), None);
        assert_eq!(manager.block(0, 1, 0), crate::level::generator::blocks::BEDROCK);
    }

    // 挖掉带 meta 的方块 元数据要跟着清零
    #[test]
    fn breaking_block_clears_meta() {
        let mut manager = ChunkManager::new();
        let _ = manager.get(0, 0);
        manager.set_block_meta(0, 5, 0, 17, 1);
        assert_eq!(manager.meta(0, 5, 0), 1);
        assert_eq!(manager.break_block(0, 5, 0), Some(17));
        assert_eq!(manager.block(0, 5, 0), 0);
        assert_eq!(manager.meta(0, 5, 0), 0);
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

    // 装饰后先不下发 完整走一遍 Init 往返验证 vanilla 光照语义
    #[test]
    fn light_and_height_after_decoration() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        let chunk = manager.get(0, 0);
        assert_eq!(chunk.height(0, 0), 4);
        // 装饰完成但光照没算
        assert!(!chunk.lit);
        assert_eq!(chunk.sky_light(0, 5, 0), 0);

        // 提交 Init → 引擎算 → 回执写回（跳过 worker 直接调引擎）
        let (tx, rx) = std::sync::mpsc::channel();
        for job in manager.take_light_jobs(&tx) {
            if let LightKind::Init(snap) = job.kind {
                let receipt = crate::level::light::engine::Engine::init(snap).into_receipt();
                manager.apply_light_receipt(receipt);
            }
        }
        assert!(rx.try_recv().is_err(), "测试不走 worker 回执应为空");

        // vanilla 语义：地表自身 0 上面的空气 15
        let chunk = manager.get(0, 0);
        assert!(chunk.lit);
        assert_eq!(chunk.sky_light(0, 5, 0), 15);
        assert_eq!(chunk.sky_light(0, CHUNK_HEIGHT - 1, 0), 15);
        assert_eq!(chunk.sky_light(0, 4, 0), 0);
        assert_eq!(chunk.sky_light(0, 3, 0), 0);
    }

    // 在途光照要算进"有活干" 否则分片会在回执到达前被撤掉
    #[test]
    fn pending_light_keeps_the_manager_busy() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        // 刚生成 待提交
        assert!(manager.has_pending_light());

        // 提交之后进在途 仍然不能算空闲
        let (tx, _rx) = std::sync::mpsc::channel();
        let jobs = manager.take_light_jobs(&tx);
        assert_eq!(jobs.len(), 1);
        assert!(manager.has_pending_light());

        // 回执写回才真正空闲
        for job in jobs {
            if let LightKind::Init(snap) = job.kind {
                let receipt = crate::level::light::engine::Engine::init(snap).into_receipt();
                manager.apply_light_receipt(receipt);
            }
        }
        assert!(!manager.has_pending_light());
    }

    // 回执捎带的邻居区块只改了边界几格 不能当它自己算完了
    #[test]
    fn receipt_does_not_open_the_gate_for_a_neighbour() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        assert!(!manager.is_lit(0, 0));

        // 目标是别家区块 只是把 (0,0) 的边界光带回来了
        let receipt = LightReceipt {
            owner: (1, 0),
            chunks: vec![crate::level::light::snapshot::ChunkLight {
                x: 0,
                z: 0,
                sky_light: vec![0; crate::level::format::chunk::NIBBLE_COUNT],
                block_light: vec![0; crate::level::format::chunk::NIBBLE_COUNT],
            }],
        };
        manager.apply_light_receipt(receipt);
        assert!(!manager.is_lit(0, 0), "没有自己的 Init 不能开下发门槛");
    }

    // Init 在途时改方块 快照里没有这次改动 必须再排一轮增量
    #[test]
    fn edit_while_init_in_flight_is_queued_for_update() {
        let mut manager = ChunkManager::new();
        assert_eq!(manager.block(0, 4, 0), GRASS);
        let (tx, _rx) = std::sync::mpsc::channel();
        let jobs = manager.take_light_jobs(&tx);
        assert_eq!(jobs.len(), 1);

        assert!(manager.set_block(0, 10, 0, 1));
        assert_eq!(
            manager.light_changes.get(&(0, 0)).map(Vec::len),
            Some(1),
            "在途期间的改动漏排了"
        );
        // 底光还没写回 现在排增量会拿旧快照算 必须先在队列里等着
        let more = manager.take_light_jobs(&tx);
        assert!(more.is_empty(), "在途期间不该排增量任务");
        assert!(manager.light_changes.contains_key(&(0, 0)));

        // 回执写回之后就能正常排了
        for job in jobs {
            if let LightKind::Init(snap) = job.kind {
                manager.apply_light_receipt(
                    crate::level::light::engine::Engine::init(snap).into_receipt(),
                );
            }
        }
        let after = manager.take_light_jobs(&tx);
        assert_eq!(after.len(), 1);
        assert!(matches!(after[0].kind, LightKind::Update { .. }));
    }

    // 改过的方块落盘后重新打开还在 没改的仍是平坦地形
    #[test]
    fn flush_persists_edits() {
        let path = temp_path("store-persist");
        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            assert!(manager.set_block(0, 5, 0, 7));
            manager.mark_all_lit_for_test();
            assert_eq!(manager.flush().unwrap(), 1);
            // 再 flush 一次没有新改动
            assert_eq!(manager.flush().unwrap(), 0);
        }

        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            assert_eq!(manager.block(0, 5, 0), 7);
            // 未修改过的区块仍然现生成
            assert_eq!(manager.block(0, 4, 0), GRASS);
            assert_eq!(manager.flush().unwrap(), 0);
        }

        let _ = std::fs::remove_file(&path);
    }

    // 装饰完成的区块必须标脏 否则 unload 时会被当无改动丢弃 下次重新生成
    #[test]
    fn populated_chunk_is_marked_dirty() {
        let mut manager = ChunkManager::new();
        let _ = manager.get(0, 0);
        assert!(manager.dirty.contains(&(0, 0)));
    }

    // 邻块稍后的装饰不能把已下发、已挖掘的区块再改一遍。
    #[test]
    fn neighbour_population_does_not_modify_completed_chunk() {
        let mut manager = ChunkManager::new();
        manager.set_generator(Generator::terrain(1337));
        let _ = manager.get(0, 0);
        manager.break_block(0, 4, 0);
        let before = manager.chunks.get(&(0, 0)).unwrap().serialize();

        for dx in -1..=1 {
            for dz in -1..=1 {
                if dx != 0 || dz != 0 {
                    let _ = manager.get(dx, dz);
                }
            }
        }

        assert_eq!(manager.chunks.get(&(0, 0)).unwrap().serialize(), before);
    }

    // 同一个区块先加载还是后加载，装饰结果必须逐格一致：
    // 旧实现读共享世界里邻居已经摆好的树，窗口平移会把同一棵树砍成两半。
    #[test]
    fn cross_chunk_decoration_is_load_order_independent() {
        const SEED: u32 = 20241115;
        let target = (1, 0);

        let mut direct = realistic_manager(SEED);
        let expected = normalized_chunk(&mut direct, target);

        // 先把西/北侧的起源区块加载并装饰，再加载目标区块。
        let mut reordered = realistic_manager(SEED);
        for (chunk_x, chunk_z) in [(-1, -1), (-1, 0), (0, -1), (0, 0)] {
            let _ = reordered.get(chunk_x, chunk_z);
        }
        assert_eq!(
            normalized_chunk(&mut reordered, target),
            expected,
            "邻块先加载后，目标区块的跨界装饰变了，树会被砍半"
        );
    }

    // region 边界只决定真实区块的属主，不能裁掉邻区贡献的跨界装饰。
    #[test]
    fn decoration_is_complete_at_region_boundary() {
        let mut unbounded = ChunkManager::new();
        unbounded.set_generator(Generator::realistic(20241115));
        let expected = {
            let chunk = unbounded.get(0, 0);
            normalize_flower_kinds(chunk);
            chunk.serialize()
        };

        let mut bounded = ChunkManager::new();
        bounded.set_generator(Generator::realistic(20241115));
        bounded.set_bounds(((0, 0), (0, 0)));
        let actual = {
            let chunk = bounded.get(0, 0);
            normalize_flower_kinds(chunk);
            chunk.serialize()
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn rtg_village_is_order_independent_and_does_not_roll_back() {
        let (seed, target, probe) = cross_chunk_village_fixture();
        let neighbours = [(target.0 - 1, target.1), (target.0, target.1 + 1)];

        // 目标区块先生成，作为 region 边界和另一种加载顺序的共同基准。
        let mut direct = realistic_manager(seed);
        let expected = normalized_chunk(&mut direct, target);

        let mut bounded = realistic_manager(seed);
        bounded.set_bounds((target, target));
        assert_eq!(
            normalized_chunk(&mut bounded, target),
            expected,
            "region 边界不能截断跨界村庄"
        );

        let mut reordered = realistic_manager(seed);
        for &(chunk_x, chunk_z) in neighbours.iter().rev() {
            let _ = reordered.get(chunk_x, chunk_z);
        }
        assert_eq!(
            normalized_chunk(&mut reordered, target),
            expected,
            "邻块先加载不能改变村庄拓扑、放置高度或人口 RNG"
        );

        // 在建筑 footprint 内挖掉当前最高方块，后续邻块人口不得把它写回来。
        let y = direct
            .get(target.0, target.1)
            .height((probe.0 & 15) as usize, (probe.1 & 15) as usize) as i32;
        assert!(direct.break_block(probe.0, y, probe.1).is_some());
        let before = direct.chunks.get(&target).unwrap().serialize();
        for (chunk_x, chunk_z) in neighbours {
            let _ = direct.get(chunk_x, chunk_z);
        }
        assert_eq!(direct.chunks.get(&target).unwrap().serialize(), before);
    }

    // 生成区块 unload 时应落盘 重新打开能读回 不再重新生成(树位置固定)
    #[test]
    fn generated_chunk_persists_after_unload() {
        let path = temp_path("gen-unload-persist");
        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            let _ = manager.get(100, 100);
            manager.mark_all_lit_for_test();
            assert!(manager.tick_force(&HashSet::new()) >= 1);
        }
        {
            let loader = ChunkLoader::open(&path).unwrap();
            assert!(
                loader.load(100, 100).unwrap().is_some(),
                "生成的区块应已落盘"
            );
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
        manager.mark_all_lit_for_test();
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
        manager.mark_all_lit_for_test();
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

    // 装饰落点从起源区块 +8 开始，树干/树冠最远能伸到起源的 +2 区块。
    // 目标区块必须把这些写入合进来，否则树会在 16 格边界上被切掉。
    #[test]
    fn decoration_writes_two_chunks_away_are_merged() {
        const SEED: u32 = 20241115;
        let mut manager = realistic_manager(SEED);

        // 先装饰一片区域，缓存里才会出现 +2 溢出的写入。
        for cx in 0..6 {
            for cz in 0..6 {
                let _ = manager.get(cx, cz);
            }
        }

        let mut spilled = Vec::new();
        for (&(origin_x, origin_z), decoration) in manager.decorations.iter() {
            for write in decoration.writes.iter() {
                if write.id == 0 || matches!(write.id, 37 | 38) {
                    continue;
                }
                let dx = (write.x >> 4) - origin_x;
                let dz = (write.z >> 4) - origin_z;
                if dx == 2 || dz == 2 {
                    spilled.push((write.x, write.y as usize, write.z, write.id));
                }
            }
        }
        spilled.sort_unstable();
        assert!(
            !spilled.is_empty(),
            "测试种子没有找到落到 +2 区块的装饰写入"
        );

        for &(x, y, z, _) in spilled.iter().take(64) {
            let chunk = manager.get(x >> 4, z >> 4);
            assert_ne!(
                chunk.block((x & 15) as usize, y, (z & 15) as usize),
                0,
                "起源区块跨两格写入的方块 ({x},{y},{z}) 没有合并进目标区块"
            );
        }
    }

    #[test]
    fn dirty_chunk_is_saved_before_unload() {
        let path = temp_path("unload-dirty");
        let (x, z) = (5 * 16 + 1, 5 * 16 + 1);
        {
            let mut manager = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
            manager.set_generator(Generator::flat());
            assert!(manager.set_block(x, 5, z, 9));
            manager.mark_all_lit_for_test();
            assert!(manager.len() > 0);
            assert!(manager.tick_force(&HashSet::new()) > 0);
            assert_eq!(manager.len(), 0);
        }

        let mut reloaded = ChunkManager::with_loader(ChunkLoader::open(&path).unwrap());
        assert_eq!(reloaded.block(x, 5, z), 9);
        let _ = std::fs::remove_file(&path);
    }

    // 从存档加载的区块必须重新排光照；空回执也要开 lit，
    // 否则区块会一直卡在下发门槛外，新地形永远刷不出来。
    #[test]
    fn loaded_chunk_waits_for_light_then_opens_on_empty_receipt() {
        let mut manager = ChunkManager::new();
        manager.insert_loaded_chunk(0, 0, Some(Chunk::empty(0, 0)));
        assert!(manager.pending_light_init.contains(&(0, 0)));
        assert!(!manager.is_lit(0, 0));

        let (tx, _rx) = std::sync::mpsc::channel();
        let jobs = manager.take_light_jobs(&tx);
        assert_eq!(jobs.len(), 1);
        assert!(manager.light_init_in_flight.contains(&(0, 0)));

        manager.apply_light_receipt(LightReceipt {
            owner: (0, 0),
            chunks: Vec::new(),
        });
        assert!(manager.is_lit(0, 0));
        assert!(manager.light_init_in_flight.is_empty());
    }
}


