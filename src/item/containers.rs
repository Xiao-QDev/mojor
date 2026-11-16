// 容器(工作台/熔炉/箱子)内容与熔炉烧制 照搬 Nukkit FurnaceInventory / BlockEntityFurnace
// 0.14 服务器按坐标持有 tile 这里放在 global region 进程内 关窗不丢 重开还在
use std::collections::HashMap;

use crate::item::inventory::{CraftingManager, fuel_ticks};
use crate::network::protocol::container::{
    ContainerSetDataPacket, ContainerSetSlotPacket,
};
use crate::network::protocol::item::ItemStack;

pub type BlockPos = (i32, i32, i32);

// 熔炉一次烧制 200 tick(10 秒) 与 Nukkit/Genisys 一致
pub const COOK_TICKS: u32 = 200;
// 燃料进度条按 200 归一
pub const BURN_BAR: u32 = 200;

// 窗口类型 与 InventoryType.getNetworkType 对应
pub const KIND_CHEST: u8 = 0;
pub const KIND_WORKBENCH: u8 = 1;
pub const KIND_FURNACE: u8 = 2;

pub fn slot_count(kind: u8) -> usize {
    match kind {
        // Nukkit InventoryType.WORKBENCH 是 10(9 个合成槽 + 1 个结果槽)
        KIND_WORKBENCH => 10,
        KIND_FURNACE => 3,
        _ => 27,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotChange {
    Smelting(ItemStack),
    Fuel(ItemStack),
    Result(ItemStack),
    // 熔炉开始/停止燃烧 客户端要看到燃烧中的方块
    Lit(bool),
}

#[derive(Debug, Clone)]
pub struct ContainerState {
    pub kind: u8,
    pub window_id: u8,
    pub slots: Vec<Option<ItemStack>>,
    // 熔炉朝向 点亮/熄火广播方块时保留
    pub meta: u8,
    // 熔炉专用
    pub burn_time: u32,
    pub max_time: u32,
    pub cook_time: u32,
    pub lit: bool,
}

impl ContainerState {
    pub fn new(kind: u8, window_id: u8) -> Self {
        Self {
            kind,
            window_id,
            slots: vec![None; slot_count(kind)],
            meta: 0,
            burn_time: 0,
            max_time: 0,
            cook_time: 0,
            lit: false,
        }
    }

    pub fn get(&self, slot: usize) -> Option<ItemStack> {
        self.slots.get(slot).copied().flatten()
    }

    pub fn set(&mut self, slot: usize, item: Option<ItemStack>) -> bool {
        let Some(cell) = self.slots.get_mut(slot) else {
            return false;
        };
        let clean = item.filter(|i| i.id > 0 && i.count > 0 && i.damage >= 0);
        if *cell != clean {
            *cell = clean;
            return true;
        }
        false
    }

    // 熔炉三格 0=待烧 1=燃料 2=产物
    fn smelting(&self) -> Option<ItemStack> {
        self.get(0)
    }

    fn fuel_item(&self) -> Option<ItemStack> {
        self.get(1)
    }

    fn result(&self) -> Option<ItemStack> {
        self.get(2)
    }

    // 能不能继续烧: 有配方 产物格空或同类且未满 64
    fn can_smelt(&self, manager: &CraftingManager) -> Option<ItemStack> {
        let raw = self.smelting()?;
        let product = manager.smelt(&raw)?;
        match self.result() {
            None => Some(product),
            Some(existing) if existing.id == product.id
                && existing.damage == product.damage
                && existing.count < 64 => Some(product),
            _ => None,
        }
    }

    // 烧完一件的产物 叠到产物格
    fn finish_smelt(&mut self, product: ItemStack, changes: &mut Vec<SlotChange>) {
        let count = self.result().map(|i| i.count).unwrap_or(0) + 1;
        self.slots[2] = Some(ItemStack {
            id: product.id,
            count,
            damage: product.damage,
        });
        changes.push(SlotChange::Result(self.slots[2].unwrap()));
        let mut raw = self.smelting().unwrap();
        raw.count = raw.count.saturating_sub(1);
        let raw = if raw.count == 0 { None } else { Some(raw) };
        self.slots[0] = raw;
        changes.push(SlotChange::Smelting(raw.unwrap_or(ItemStack {
            id: 0,
            count: 0,
            damage: 0,
        })));
    }

    // 每 tick 推进一次烧制 返回需要下发给观察者的变更
    pub fn tick(&mut self, manager: &CraftingManager) -> Vec<SlotChange> {
        if self.kind != KIND_FURNACE {
            return Vec::new();
        }
        let mut changes = Vec::new();
        let can_smelt = self.can_smelt(manager);

        // 燃料烧完且还能烧 就吃一格新燃料 照 Nukkit checkFuel
        if self.burn_time == 0 && can_smelt.is_some() {
            if let Some(mut fuel) = self.fuel_item() {
                if let Some(ticks) = fuel_ticks(fuel.id) {
                    self.max_time = ticks;
                    self.burn_time = ticks;
                    fuel.count = fuel.count.saturating_sub(1);
                    let remaining = if fuel.count == 0 { None } else { Some(fuel) };
                    self.slots[1] = remaining;
                    changes.push(SlotChange::Fuel(remaining.unwrap_or(ItemStack {
                        id: 0,
                        count: 0,
                        damage: 0,
                    })));
                }
            }
        }

        if self.burn_time > 0 {
            self.burn_time -= 1;
            if can_smelt.is_some() {
                self.cook_time += 1;
                if self.cook_time >= COOK_TICKS {
                    self.finish_smelt(can_smelt.unwrap(), &mut changes);
                    self.cook_time -= COOK_TICKS;
                }
            } else {
                self.cook_time = 0;
            }
            if !self.lit {
                self.lit = true;
                changes.push(SlotChange::Lit(true));
            }
        } else {
            // 熄火
            self.cook_time = 0;
            if self.lit {
                self.lit = false;
                changes.push(SlotChange::Lit(false));
            }
        }
        changes
    }

    // 进度条两条 Nukkit property 0=CookTime 1=BurnDuration
    pub fn progress_packets(&self) -> Vec<ContainerSetDataPacket> {
        if self.kind != KIND_FURNACE {
            return Vec::new();
        }
        let burn_bar = if self.max_time == 0 {
            0
        } else {
            self.burn_time * BURN_BAR / self.max_time
        };
        vec![
            ContainerSetDataPacket {
                window_id: self.window_id,
                property: 0,
                value: self.cook_time.min(u16::MAX as u32) as u16,
            },
            ContainerSetDataPacket {
                window_id: self.window_id,
                property: 1,
                value: burn_bar.min(u16::MAX as u32) as u16,
            },
        ]
    }
}

#[derive(Debug, Default)]
pub struct ContainerRegistry {
    pub blocks: HashMap<BlockPos, ContainerState>,
    // 玩家当前打开的容器坐标
    pub opened: HashMap<std::net::SocketAddr, BlockPos>,
    // 下一个可用窗口 id 从 1 开始递增 与 Nukkit Player::addWindow 一致
    // 固定用 1 会让不同容器撞车 连点还会重复下发 ContainerOpen 把客户端窗口表打崩
    next_window: u8,
}

impl ContainerRegistry {
    // 返回 (容器, 是否已经处于打开状态)
    // 已经打开时调用方不要再发 ContainerOpen 只刷新内容 重复开窗会让客户端闪退
    pub fn open(
        &mut self,
        addr: std::net::SocketAddr,
        pos: BlockPos,
        kind: u8,
        meta: u8,
    ) -> (&mut ContainerState, bool) {
        let already = self.opened.get(&addr) == Some(&pos) && self.blocks.contains_key(&pos);
        self.opened.insert(addr, pos);
        let window_id = if already {
            self.blocks[&pos].window_id
        } else {
            // 每个容器一个稳定窗口 id 与 Nukkit addWindow 的递增分配一致
            if self.next_window == 0 {
                self.next_window = 1;
            }
            let id = self.next_window;
            self.next_window = self.next_window.wrapping_add(1).max(1);
            id
        };
        let state = self
            .blocks
            .entry(pos)
            .or_insert_with(|| ContainerState::new(kind, window_id));
        if kind == KIND_FURNACE {
            state.meta = meta;
        }
        (state, already)
    }

    pub fn close(&mut self, addr: std::net::SocketAddr) {
        self.opened.remove(&addr);
    }

    pub fn pos_of(&self, addr: std::net::SocketAddr) -> Option<BlockPos> {
        self.opened.get(&addr).copied()
    }

    // 客户端拖动容器槽位
    pub fn set_slot(
        &mut self,
        addr: std::net::SocketAddr,
        slot: usize,
        item: Option<ItemStack>,
    ) -> bool {
        let Some(pos) = self.pos_of(addr) else {
            return false;
        };
        self.blocks
            .get_mut(&pos)
            .is_some_and(|state| state.set(slot, item))
    }
}

// 单槽更新包 与 ContainerSetContent 的区别是只动一格
pub fn slot_packet(window_id: u8, slot: usize, item: Option<ItemStack>) -> ContainerSetSlotPacket {
    ContainerSetSlotPacket {
        window_id,
        slot: slot as i16,
        hotbar_slot: 0,
        item,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn furnace() -> ContainerState {
        ContainerState::new(KIND_FURNACE, 1)
    }

    fn stack(id: i16, count: u8) -> ItemStack {
        ItemStack { id, count, damage: 0 }
    }

    #[test]
    fn furnace_smelts_after_200_ticks() {
        let manager = CraftingManager::new();
        let mut f = furnace();
        f.set(0, Some(stack(15, 1))); // 铁矿
        f.set(1, Some(stack(263, 1))); // 煤炭
        let mut finished = false;
        for _ in 0..COOK_TICKS + 2 {
            for change in f.tick(&manager) {
                if matches!(change, SlotChange::Result(_)) {
                    finished = true;
                }
            }
        }
        assert!(finished);
        assert_eq!(f.get(2).map(|i| i.id), Some(265)); // 铁锭
        assert!(f.get(0).is_none());
        assert!(f.get(1).is_none());
        assert!(f.lit);
    }

    #[test]
    fn furnace_without_fuel_stays_cold() {
        let manager = CraftingManager::new();
        let mut f = furnace();
        f.set(0, Some(stack(15, 3)));
        for _ in 0..10 {
            f.tick(&manager);
        }
        assert_eq!(f.cook_time, 0);
        assert!(!f.lit);
        assert!(f.get(2).is_none());
    }

    #[test]
    fn unsmeltable_input_never_burns_fuel() {
        let manager = CraftingManager::new();
        let mut f = furnace();
        f.set(0, Some(stack(1, 1))); // 石头 没有配方
        f.set(1, Some(stack(263, 1)));
        for _ in 0..10 {
            f.tick(&manager);
        }
        assert_eq!(f.get(1).map(|i| i.count), Some(1));
        assert_eq!(f.burn_time, 0);
    }

    #[test]
    fn container_slots_persist_and_are_addressable() {
        let mut reg = ContainerRegistry::default();
        let addr: std::net::SocketAddr = "127.0.0.1:60000".parse().unwrap();
        reg.open(addr, (1, 2, 3), KIND_CHEST, 0);
        assert!(reg.set_slot(addr, 0, Some(stack(4, 5))));
        reg.close(addr);
        assert_eq!(reg.blocks[&(1, 2, 3)].get(0), Some(stack(4, 5)));
        // 关窗后不再接受这个玩家的槽位更新
        assert!(!reg.set_slot(addr, 1, Some(stack(4, 1))));
    }
}
