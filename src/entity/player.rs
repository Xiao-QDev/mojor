use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::RwLock;

use bytes::Bytes;

use crate::entity::SPAWN_POSITION;
use crate::network::protocol::add_player::AddPlayerPacket;
use crate::network::protocol::container_set_content::ContainerSetContentPacket;
use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::login::Skin;
use crate::network::protocol::player_list::PlayerListEntry;

#[derive(Clone)]
pub struct PlayerState {
    pub(crate) logged_in: bool,
    // 出生状态已下发 之后参与互相可见
    pub(crate) spawned: bool,
    pub(crate) eid: i64,
    pub(crate) uuid: [u8; 16],
    pub(crate) username: String,
    pub(crate) skin: Skin,
    // 区块队列清空后由 region 通知 再补出生状态
    pub(crate) spawn_pending: bool,
    pub(crate) position: (f32, f32, f32),
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    // 背包 27 个主格
    pub(crate) inventory: Vec<Option<ItemStack>>,
    // 快捷栏位置 -> 包内槽位 + 9 与 Nukkit 的换算一致
    pub(crate) hotbar: Vec<i32>,
    pub(crate) sneaking: bool,
    pub(crate) sprinting: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            logged_in: false,
            spawned: false,
            // 0 保留给客户端自己的实体 StartGame 里固定为 0
            eid: 0,
            uuid: [0; 16],
            username: String::new(),
            skin: Skin {
                model: String::new(),
                data: Bytes::new(),
            },
            spawn_pending: false,
            position: SPAWN_POSITION,
            yaw: 0.0,
            pitch: 0.0,
            inventory: vec![None; 27],
            hotbar: (0..9).map(|slot| slot + 9).collect(),
            sneaking: false,
            sprinting: false,
        }
    }
}

// 玩家摘要 由 global region 维护 相当于 Folia 的 player list
// region 线程不持有别的玩家的完整状态 只读这份快照做可见性与拾取判定
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct PlayerSnapshot {
    pub(crate) eid: i64,
    pub(crate) uuid: [u8; 16],
    pub(crate) username: String,
    pub(crate) skin: Skin,
    pub(crate) position: (f32, f32, f32),
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) spawned: bool,
}

impl PlayerSnapshot {
    pub(crate) fn from_state(state: &PlayerState) -> Self {
        Self {
            eid: state.eid,
            uuid: state.uuid,
            username: state.username.clone(),
            skin: state.skin.clone(),
            position: state.position,
            yaw: state.yaw,
            pitch: state.pitch,
            spawned: state.spawned,
        }
    }
}

// 全局玩家目录 写只在 global region 的 tick 边界发生 region 线程只读
// 这是 chunk watcher 列表的等价物 是唯一允许跨 region 共享的数据
pub(crate) struct PlayerDirectory {
    inner: RwLock<HashMap<SocketAddr, PlayerSnapshot>>,
}

impl PlayerDirectory {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn replace(&self, entries: Vec<(SocketAddr, PlayerSnapshot)>) {
        let mut map = self.inner.write().unwrap_or_else(|e| e.into_inner());
        map.clear();
        for (addr, snapshot) in entries {
            map.insert(addr, snapshot);
        }
    }

    // 已出生玩家的快照 按地址排序保证各 region 看到一致的顺序
    pub(crate) fn spawned(&self) -> Vec<(SocketAddr, PlayerSnapshot)> {
        let map = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<_> = map
            .iter()
            .filter(|(_, state)| state.spawned)
            .map(|(addr, state)| (*addr, state.clone()))
            .collect();
        out.sort_by_key(|(addr, _)| *addr);
        out
    }

    // 视野范围内的已出生玩家 广播按视野展开就不再全服扫一遍
    pub(crate) fn nearby(
        &self,
        center: (f32, f32, f32),
        radius_blocks: f32,
    ) -> Vec<(SocketAddr, PlayerSnapshot)> {
        let radius_sq = radius_blocks * radius_blocks;
        let map = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<_> = map
            .iter()
            .filter(|(_, state)| {
                if !state.spawned {
                    return false;
                }
                let dx = state.position.0 - center.0;
                let dy = state.position.1 - center.1;
                let dz = state.position.2 - center.2;
                dx * dx + dy * dy + dz * dz <= radius_sq
            })
            .map(|(addr, state)| (*addr, state.clone()))
            .collect();
        out.sort_by_key(|(addr, _)| *addr);
        out
    }
}

// 0.14 的实体标志位 与 Nukkit Entity 的 DATA_FLAG_* 对应
pub const FLAG_SNEAKING: u8 = 0x02;
pub const FLAG_SPRINTING: u8 = 0x08;

impl PlayerState {
    // 姿态标志 客户端据此渲染下蹲与疾跑
    pub(crate) fn entity_flags(&self) -> u8 {
        let mut flags = 0;
        if self.sneaking {
            flags |= FLAG_SNEAKING;
        }
        if self.sprinting {
            flags |= FLAG_SPRINTING;
        }
        flags
    }

    pub(crate) fn inventory_packet(&self) -> ContainerSetContentPacket {
        ContainerSetContentPacket::inventory(self.inventory.clone(), self.hotbar.clone())
    }

    // 先叠同类再找空位 放不下返回 false
    pub(crate) fn add_item(&mut self, item: ItemStack) -> bool {
        for slot in self.inventory.iter_mut().flatten() {
            if slot.id == item.id && slot.damage == item.damage && slot.count < 64 {
                slot.count = (slot.count + item.count).min(64);
                return true;
            }
        }
        for slot in self.inventory.iter_mut() {
            if slot.is_none() {
                *slot = Some(item);
                return true;
            }
        }
        false
    }

    // 扣掉一个指定方块 返回是否扣成功
    pub(crate) fn consume_item(&mut self, block_id: u8) -> bool {
        let Some(index) = self
            .inventory
            .iter()
            .position(|slot| slot.is_some_and(|item| item.id == block_id as i16 && item.count > 0))
        else {
            return false;
        };

        let Some(item) = self.inventory[index].as_mut() else {
            return false;
        };
        item.count -= 1;
        if item.count == 0 {
            self.inventory[index] = None;
        }
        true
    }

    pub(crate) fn list_entry(&self) -> PlayerListEntry {
        PlayerListEntry {
            uuid: self.uuid,
            eid: self.eid,
            username: self.username.clone(),
            skin: self.skin.clone(),
        }
    }

    pub(crate) fn add_packet(&self) -> Bytes {
        AddPlayerPacket {
            uuid: self.uuid,
            username: self.username.clone(),
            eid: self.eid,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            metadata: Metadata::player(&self.username),
        }
        .encode()
    }
}

// 玩家在 region 里的完整状态 Folia 的 localPlayers 一项
// global region 只留一份只读镜像 权威状态与区块视野都由属主分片独占
#[derive(Clone)]
pub(crate) struct RegionPlayer {
    pub(crate) state: PlayerState,
    pub(crate) view: PlayerView,
}

impl RegionPlayer {
    pub(crate) fn new(state: PlayerState, radius: i32) -> Self {
        let view = PlayerView::new(state.position, radius);
        Self { state, view }
    }
}

// 区块下发视图 由 region 线程独占
// 网络层只负责把玩家的位置变化推过来 队列怎么推进全在 region 里
#[derive(Clone)]
pub(crate) struct PlayerView {
    pub(crate) position: (f32, f32, f32),
    pub(crate) chunk_center: (i32, i32),
    // 视野半径 以区块为单位
    pub(crate) radius: i32,
    // 已下发过的区块 移动时只补新进入视野的部分
    pub(crate) sent_chunks: HashSet<(i32, i32)>,
    pub(crate) pending_chunks: VecDeque<(i32, i32)>,
    // 已经投给别的 region 代发的区块数 等对方回执到了才算交付完
    pub(crate) pending_foreign: usize,
    // 已经交给 IO 池在后台加载的区块数 回执到了重新入队才算交付完
    pub(crate) pending_load: usize,
    // 队列刚清空还没通知过外层
    pub(crate) completed: bool,
    // 上一次为它做加载圈预取时的玩家区块中心 没跨区块就不重复发预取消息
    pub(crate) prefetch_center: Option<(i32, i32)>,
}

// 下发圈照搬 vanilla ChunkTrackingView.isWithinDistance
// 欧氏圆 但先减一格 buffer 边缘区块只加载不下发 省带宽也不闪
// 加载圈(Chebyshev 方形)在 level::distance 的距离场里 两处形状刻意不同
pub(crate) fn within_view_distance(
    center: (i32, i32),
    chunk: (i32, i32),
    view_distance: i32,
) -> bool {
    const BUFFER: i64 = 1;
    let dx = ((chunk.0 - center.0).abs() as i64 - BUFFER).max(0);
    let dz = ((chunk.1 - center.1).abs() as i64 - BUFFER).max(0);
    dx * dx + dz * dz < (view_distance as i64) * (view_distance as i64)
}

impl PlayerView {
    pub(crate) fn new(position: (f32, f32, f32), radius: i32) -> Self {
        let chunk_center = (
            (position.0.floor() as i32) >> 4,
            (position.2.floor() as i32) >> 4,
        );
        let mut view = Self {
            position,
            chunk_center,
            radius,
            sent_chunks: HashSet::new(),
            pending_chunks: VecDeque::new(),
            pending_foreign: 0,
            pending_load: 0,
            completed: false,
            prefetch_center: None,
        };
        view.queue_around(chunk_center, radius);
        view
    }

    // 排队以 center 为中心视野内的区块 下发圈之外照不到 已经下发过的跳过
    pub(crate) fn queue_around(&mut self, center: (i32, i32), radius: i32) {
        for chunk_x in center.0 - radius..=center.0 + radius {
            for chunk_z in center.1 - radius..=center.1 + radius {
                if !within_view_distance(center, (chunk_x, chunk_z), radius) {
                    continue;
                }
                if self.sent_chunks.insert((chunk_x, chunk_z)) {
                    self.pending_chunks.push_back((chunk_x, chunk_z));
                }
            }
        }
        // 视野外留两圈缓冲之外的已下发记录直接丢掉
        // 否则走过的区块坐标会一直留在集合里 玩家探索越久内存越大
        self.sent_chunks
            .retain(|&(x, z)| within_view_distance(center, (x, z), radius + 2));
        self.completed = false;
    }

    // 位置变了 跨区块时补发新视野
    pub(crate) fn update(&mut self, position: (f32, f32, f32), radius: i32) {
        self.position = position;
        let chunk = (
            (position.0.floor() as i32) >> 4,
            (position.2.floor() as i32) >> 4,
        );
        if chunk != self.chunk_center {
            self.chunk_center = chunk;
            self.queue_around(chunk, radius);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 半径 2 时下发圈是完整的 5x5 25 块 与 vanilla 的 ChunkTrackingView 一致
    #[test]
    fn radius_two_covers_the_full_square() {
        let mut count = 0;
        for x in -2..=2 {
            for z in -2..=2 {
                assert!(
                    within_view_distance((0, 0), (x, z), 2),
                    "({x},{z}) 应在半径 2 的下发圈内"
                );
                count += 1;
            }
        }
        assert_eq!(count, 25);
        // 圈外一格
        assert!(!within_view_distance((0, 0), (3, 0), 2));
        assert!(!within_view_distance((0, 0), (3, 1), 2));
    }

    // 半径 3 时 buffer 内缩后角上的平方和 8 < 9 仍含 下发圈还是完整 7x7
    // buffer 要视距更大才显出切角效果 见下一个用例
    #[test]
    fn radius_three_still_covers_the_square() {
        let count = (-3..=3)
            .flat_map(|x| (-3..=3).map(move |z| (x, z)))
            .filter(|&(x, z)| within_view_distance((0, 0), (x, z), 3))
            .count();
        assert_eq!(count, 49);
        assert!(within_view_distance((0, 0), (3, 3), 3));
    }

    // 半径 4 时切掉四角 delta 3+3 平方和 18 >= 16 欧氏圆的形状才显出来
    #[test]
    fn radius_four_clips_the_corners() {
        let count = (-4..=4)
            .flat_map(|x| (-4..=4).map(move |z| (x, z)))
            .filter(|&(x, z)| within_view_distance((0, 0), (x, z), 4))
            .count();
        // 9x9 去掉四角
        assert_eq!(count, 81 - 4);
        assert!(within_view_distance((0, 0), (4, 3), 4));
        assert!(!within_view_distance((0, 0), (4, 4), 4));
        assert!(within_view_distance((0, 0), (4, 0), 4));
    }

    // 半径 1 时 3x3 全发 减完 buffer delta 全是 0
    #[test]
    fn radius_one_sends_all_nine() {
        let count = (-1..=1)
            .flat_map(|x| (-1..=1).map(move |z| (x, z)))
            .filter(|&(x, z)| within_view_distance((0, 0), (x, z), 1))
            .count();
        assert_eq!(count, 9);
    }

    // 视野整体平移 判定跟着中心走
    #[test]
    fn circle_follows_the_center() {
        assert!(within_view_distance((10, 10), (12, 10), 2));
        assert!(!within_view_distance((10, 10), (13, 10), 2));
        assert!(within_view_distance((10, 10), (8, 10), 2));
    }
}
