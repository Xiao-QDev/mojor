use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bytes::Bytes;

use crate::config::DEFAULT_VIEW_DISTANCE;
use crate::entity::{PlayerDirectory, RegionPlayer};
use crate::error::Result;
use crate::level::actor::{IDLE_SHUTDOWN_TICKS, RegionHandle, RegionOutbound, RegionRegistry};
use crate::level::region_handler::PacketResult;
use crate::level::chunk_io::ChunkIoPool;
use crate::level::chunk_loader::ChunkLoader;
use crate::level::generator::Generator;
use crate::level::region::RegionId;



// 世界入口 Folia 式分片：区块按 region 归组 每个 region 一套缓存与生成器
// 一个 region 对应一个 mjr 文件 落盘与缓存都以 region 为单位
pub struct Level {
    loader: Option<Arc<ChunkLoader>>,
    // 新 region 建立时按这个模板克隆生成器
    template: Generator,
    // 每个分片一个独占线程 区块状态只在那个线程里动
    regions: HashMap<RegionId, RegionHandle>,
    // 分片之间的投递通道表 让 region 线程能找到对方
    registry: Arc<RegionRegistry>,
    // 全局玩家目录 由 global region 写 各分片只读
    directory: Arc<PlayerDirectory>,
    // 分片连续空闲多少 tick 后上报
    idle_ticks: u64,
    // 可见半径换算用的视距 与网络层一致 启动时定死
    view_distance: i32,
    // 全局共享的区块 I/O 线程池 把同步读盘挪出 region 线程
    io: Arc<ChunkIoPool>,
}

impl Level {
    pub fn new() -> Self {
        Self {
            loader: None,
            template: Generator::default(),
            regions: HashMap::new(),
            registry: Arc::new(RegionRegistry::new()),
            directory: Arc::new(PlayerDirectory::new()),
            idle_ticks: IDLE_SHUTDOWN_TICKS,
            view_distance: DEFAULT_VIEW_DISTANCE,
            io: ChunkIoPool::new(None, 1),
        }
    }

    // 带存档的世界 区块缺失时从磁盘加载 io_workers 是后台加载线程数
    pub fn with_loader(loader: ChunkLoader, io_workers: usize) -> Self {
        let arc = Arc::new(loader);
        Self {
            loader: Some(arc.clone()),
            io: ChunkIoPool::new(Some(arc), io_workers),
            ..Self::new()
        }
    }

    // 换地形生成方式 已有 region 也会跟着换
    pub fn set_generator(&mut self, generator: Generator) {
        self.template = generator.clone();
        for handle in self.regions.values() {
            handle.set_generator(generator.clone());
        }
    }

    // 视距启动时定死 要在第一个分片建起来之前设好
    pub(crate) fn set_view_distance(&mut self, view_distance: i32) {
        self.view_distance = view_distance;
    }

    // 取本分片的执行单元 不在就按模板起一个线程
    fn region(&mut self, id: RegionId) -> &RegionHandle {
        if !self.regions.contains_key(&id) {
            let handle = RegionHandle::spawn(
                id,
                self.loader.clone(),
                self.template.clone(),
                self.registry.clone(),
                self.directory.clone(),
                self.idle_ticks,
                self.view_distance,
                self.io.clone(),
            );
            self.regions.insert(id, handle);
        }
        self.regions.get(&id).expect("分片刚刚建立")
    }

    pub fn block(&mut self, x: i32, y: i32, z: i32) -> u8 {
        self.region(RegionId::of_block(x, z)).block(x, y, z)
    }

    // 只读缓存 未命中返回 None 不在 global tick 上同步读盘
    pub fn payload(&mut self, chunk_x: i32, chunk_z: i32) -> Option<Bytes> {
        self.region(RegionId::of_chunk(chunk_x, chunk_z))
            .payload(chunk_x, chunk_z)
    }

    pub fn highest_block(&mut self, x: i32, z: i32) -> i32 {
        self.region(RegionId::of_block(x, z)).highest_block(x, z)
    }

    // 一批列高按分片分组 出生点选点一次只要几条消息
    pub fn highest_blocks(&mut self, columns: &[(i32, i32)]) -> Vec<i32> {
        let mut out = vec![0i32; columns.len()];
        let mut groups: HashMap<RegionId, Vec<usize>> = HashMap::new();
        for (index, (x, z)) in columns.iter().enumerate() {
            groups
                .entry(RegionId::of_block(*x, *z))
                .or_default()
                .push(index);
        }
        for (id, indices) in groups {
            let subset: Vec<(i32, i32)> = indices.iter().map(|&i| columns[i]).collect();
            let values = self.region(id).highest_blocks(&subset);
            for (index, value) in indices.into_iter().zip(values) {
                out[index] = value;
            }
        }
        out
    }

    // 存档句柄 后台刷盘线程需要它
    pub fn loader(&self) -> Option<Arc<ChunkLoader>> {
        self.loader.clone()
    }

    // 把已经写进 page cache 的内容刷到磁盘
    pub fn sync(&self) -> Result<()> {
        match &self.loader {
            Some(loader) => loader.sync(),
            None => Ok(()),
        }
    }

    // 每个 region 各自落盘 一个文件对应一片区域
    pub fn flush(&mut self) -> Result<usize> {
        let mut total = 0;
        for handle in self.regions.values() {
            total += handle.flush();
        }
        Ok(total)
    }

    // 周期保存走异步 只投递不等 region 自己分批写 不阻塞 global tick
    pub fn flush_async(&mut self) {
        for handle in self.regions.values() {
            handle.flush_async();
        }
    }

    pub fn len(&self) -> usize {
        self.regions.values().map(|handle| handle.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.values().all(|handle| handle.len() == 0)
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    // 玩家的区块下发视图交给所在 region
    pub(crate) fn join(&mut self, addr: std::net::SocketAddr, player: RegionPlayer) {
        // 视野跨到的分片都要先建好 不然跨界区块没人能生成
        self.ensure_view_regions(player.view.chunk_center, player.view.radius);
        let (x, z) = (player.state.position.0, player.state.position.2);
        let id = RegionId::of_block(x.floor() as i32, z.floor() as i32);
        self.region(id).join(addr, player);
    }

    pub(crate) fn leave(&mut self, addr: std::net::SocketAddr, id: RegionId) -> bool {
        self.region(id).leave(addr)
    }

    // 玩家跨区前先把目标位置视野涉及的分片都建好
    pub(crate) fn ensure_view(&mut self, position: (f32, f32, f32), radius: i32) {
        let center = (
            (position.0.floor() as i32) >> 4,
            (position.2.floor() as i32) >> 4,
        );
        self.ensure_view_regions(center, radius);
    }

    // 视野覆盖到的分片全部建起来
    // 跨界的区块要由属主分片生成 属主不存在就没人能出数据
    // 与距离场的加载圈同形 Chebyshev 方形 边缘多建几个分片让 GC 去回收
    fn ensure_view_regions(&mut self, center: (i32, i32), radius: i32) {
        let mut ids = Vec::new();
        for chunk_x in center.0 - radius..=center.0 + radius {
            for chunk_z in center.1 - radius..=center.1 + radius {
                let id = RegionId::of_chunk(chunk_x, chunk_z);
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        for id in ids {
            let _ = self.region(id);
        }
    }

    pub(crate) fn directory(&self) -> Arc<PlayerDirectory> {
        self.directory.clone()
    }

    // 测试把空闲阈值调小 不用真等 10 秒
    // 业务 GC 第三级 回收不可达分片的文件句柄
    pub(crate) fn gc_files(&self, live: &HashSet<RegionId>) -> usize {
        let Some(loader) = &self.loader else {
            return 0;
        };
        let keys: HashSet<(i32, i32)> = live.iter().map(|id| (id.x, id.z)).collect();
        loader.gc_files(&keys)
    }

    // 业务 GC 把玩家视野算出来的可达区块投给每个分片
    pub(crate) fn run_gc(&mut self, roots: &HashMap<RegionId, HashSet<(i32, i32)>>) {
        let ids: Vec<RegionId> = self.regions.keys().copied().collect();
        for id in ids {
            let live = roots.get(&id).cloned().unwrap_or_default();
            self.region(id).gc(live);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_idle_ticks_for_test(&mut self, ticks: u64) {
        self.idle_ticks = ticks;
    }

    // 撤掉空闲分片 先把脏块落盘 再摘投递通道 最后 drop 触发 Stop 与 join
    #[cfg(test)]
    pub(crate) fn has_region(&self, id: RegionId) -> bool {
        self.regions.contains_key(&id)
    }

    pub(crate) fn remove_region(&mut self, id: RegionId) -> bool {
        let Some(handle) = self.regions.remove(&id) else {
            return false;
        };
        let _ = handle.flush();
        self.registry.unregister(id);
        drop(handle);
        true
    }

    // 所有玩家都退了就立刻撤掉全部分片 区块缓存与线程一起释放
    // 不等空闲判定 退出即回收 内存尽快回到冷启动基线
    pub(crate) fn remove_all_regions(&mut self) {
        let ids: Vec<RegionId> = self.regions.keys().copied().collect();
        for id in ids {
            self.remove_region(id);
        }
    }

    // 按玩家归属把包投进分片 返回分片产出的下行与玩家状态
    pub(crate) fn handle_packet(
        &mut self,
        addr: std::net::SocketAddr,
        id: RegionId,
        packet: &[u8],
        gamemode: u8,
        view_distance: i32,
    ) -> PacketResult {
        self.region(id)
            .handle_packet(addr, packet, gamemode, view_distance)
    }

    #[cfg(test)]
    pub(crate) fn place_block_for(
        &mut self,
        addr: std::net::SocketAddr,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        id: u8,
    ) {
        self.region(RegionId::of_block(target.0, target.2))
            .place_block_for(addr, click, target, id);
    }

    pub(crate) fn sync_player(
        &mut self,
        addr: std::net::SocketAddr,
        id: RegionId,
        state: crate::entity::PlayerState,
    ) {
        self.region(id).sync_player(addr, state);
    }

    // 掉落物交给方块属主分片生成
    pub(crate) fn add_drop(
        &mut self,
        x: i32,
        z: i32,
        eid: i64,
        item: crate::network::protocol::item::ItemStack,
        block: (i32, i32, i32),
    ) {
        self.region(RegionId::of_block(x, z))
            .add_drop(eid, item, block);
    }

    // 玩家出生 让所有分片把已有的掉落物补发给他
    pub(crate) fn send_drops(&mut self, addr: std::net::SocketAddr) {
        let ids: Vec<RegionId> = self.regions.keys().copied().collect();
        for id in ids {
            self.region(id).send_drops(addr);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_drop_age(&mut self, id: RegionId, index: usize, age: u32) {
        self.region(id).set_drop_age(index, age);
    }

    // 把各 region 这一轮的产出收上来
    pub fn drain_outbound(&mut self) -> Vec<RegionOutbound> {
        let mut out = Vec::new();
        for handle in self.regions.values() {
            out.extend(handle.drain_outbound());
        }
        out
    }
}

impl Default for Level {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::generator::flat::{self, GRASS};

    // 写方块要走属主分片 等事件回执
    fn place_and_wait(
        level: &mut Level,
        click: (i32, i32, i32),
        target: (i32, i32, i32),
        id: u8,
    ) -> bool {
        let addr: std::net::SocketAddr = "127.0.0.1:50097".parse().unwrap();
        level.place_block_for(addr, click, target, id);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            for item in level.drain_outbound() {
                for event in item.events {
                    if let crate::level::actor::RegionEvent::BlockPlaced { ok, .. } = event {
                        return ok;
                    }
                }
            }
            assert!(std::time::Instant::now() < deadline, "放置没有回执");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn loads_and_edits_blocks() {
        let mut level = Level::new();
        // 取 region 中间的位置 周围一圈都在同一片区域内
        assert_eq!(level.block(100, 4, 100), GRASS);
        // 点草方块的上表面 往上面放一块石头
        assert!(place_and_wait(&mut level, (100, 4, 100), (100, 5, 100), 3));
        assert_eq!(level.block(100, 5, 100), 3);
        assert_eq!(level.len(), 9);
        assert_eq!(level.region_count(), 1);
    }

    // 世界原点正好在 region 角落 越界的那半边不归本分片管
    #[test]
    fn region_border_does_not_cross() {
        let mut level = Level::new();
        assert_eq!(level.block(0, 4, 0), GRASS);
        assert_eq!(level.len(), 4);
    }

    #[test]
    fn payload_matches_edits() {
        let mut level = Level::new();
        // 先触发生成让区块进缓存 payload 现在只读缓存不触发读盘
        assert_eq!(level.block(0, 4, 0), GRASS);
        assert_eq!(level.payload(0, 0).expect("区块已生成")[5], 0);
        place_and_wait(&mut level, (0, 4, 0), (0, 5, 0), 3);
        assert_eq!(level.payload(0, 0).expect("区块已生成")[5], 3);
    }

    #[test]
    fn generates_flat_layers() {
        let mut level = Level::new();
        assert_eq!(level.block(0, 0, 0), flat::BEDROCK);
        assert_eq!(level.block(0, 1, 0), flat::DIRT);
        assert_eq!(level.block(0, 4, 0), GRASS);
    }

    // 相隔 512 格的方块属于不同 region 各自成片
    #[test]
    fn far_blocks_live_in_different_regions() {
        let mut level = Level::new();
        assert!(place_and_wait(&mut level, (0, 4, 0), (0, 5, 0), 1));
        assert!(place_and_wait(&mut level, (600, 4, 0), (600, 5, 0), 1));
        assert_eq!(level.region_count(), 2);
    }
}

#[cfg(test)]
mod probe {
    use super::*;
    use crate::level::generator::Generator;

    #[test]
    #[ignore]
    fn timing() {
        let t = std::time::Instant::now();
        let mut level = Level::new();
        level.set_generator(Generator::flat());
        for cx in -2..=2 {
            for cz in -2..=2 {
                let _ = level.payload(cx, cz);
            }
        }
        println!("flat 25 payload: {:?}", t.elapsed());

        let t = std::time::Instant::now();
        let mut level = Level::new();
        level.set_generator(Generator::terrain(1337));
        for cx in -2..=2 {
            for cz in -2..=2 {
                let _ = level.payload(cx, cz);
            }
        }
        println!("terrain 25 payload: {:?}", t.elapsed());

        let t = std::time::Instant::now();
        let mut manager = crate::level::chunk_manager::ChunkManager::new();
        manager.set_generator(Generator::terrain(1337));
        for cx in 0..4 {
            for cz in 0..4 {
                manager.payload(cx, cz);
            }
        }
        println!("manager 16 chunks: {:?}", t.elapsed());

        let t = std::time::Instant::now();
        let mut level = Level::new();
        level.set_generator(Generator::terrain(1337));
        for cx in 0..4 {
            for cz in 0..4 {
                let _ = level.payload(cx, cz);
            }
        }
        println!("16 chunks warm: {:?}", t.elapsed());

        let t = std::time::Instant::now();
        for i in 0..50000 {
            level.block(i % 64, 64, (i / 64) % 64);
        }
        println!("50k block reads warm: {:?}", t.elapsed());
    }
}
