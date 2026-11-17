// 背包与合成 照搬 Genisys 0.14.3 pocketmine/inventory 与 example/inventory
// InventoryType.php 容器类型与尺寸 BaseInventory.php 槽位读写
// ShapedRecipe/ShapelessRecipe/FurnaceRecipe.php 配方结构
// CraftingManager.php 配方表(recipes.json) Fuel.php 燃料时长
use crate::item::ids;
use crate::network::protocol::item::ItemStack;

pub const MAX_STACK_SIZE: u8 = 64;

// 容器类型 InventoryType.php 的 init() 表
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryType {
    Chest,
    DoubleChest,
    Player,
    Furnace,
    Crafting,
    Workbench,
    BrewingStand,
    Anvil,
    EnchantTable,
    Dispenser,
    Dropper,
    Hopper,
    PlayerFloating,
}

impl InventoryType {
    // PHP 里的类型编号 客户端窗口也按这个区分
    pub const fn type_id(self) -> u8 {
        match self {
            Self::Chest => 0,
            Self::DoubleChest => 1,
            Self::Player => 2,
            Self::Furnace => 3,
            Self::Crafting => 4,
            Self::Workbench => 5,
            Self::BrewingStand => 7,
            Self::Anvil => 8,
            Self::EnchantTable => 9,
            Self::Dispenser => 10,
            Self::Dropper => 11,
            Self::Hopper => 12,
            Self::PlayerFloating => 254,
        }
    }

    pub const fn size(self) -> usize {
        match self {
            Self::Chest => 27,
            Self::DoubleChest => 54,
            Self::Player => 40, // 36 容器 + 4 盔甲
            Self::Furnace => 3,
            Self::Crafting => 5,   // 4 合成格 + 1 结果
            Self::Workbench => 10, // 9 合成格 + 1 结果
            Self::EnchantTable => 2,
            Self::BrewingStand => 4,
            Self::Anvil => 3,
            Self::Dispenser | Self::Dropper => 9,
            Self::Hopper => 5,
            Self::PlayerFloating => 36,
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Chest | Self::DoubleChest => "Chest",
            Self::Player => "Player",
            Self::Furnace => "Furnace",
            Self::Crafting | Self::Workbench => "Crafting",
            Self::BrewingStand => "Brewing",
            Self::Anvil => "Anvil",
            Self::EnchantTable => "Enchant",
            Self::Dispenser => "Dispenser",
            Self::Dropper => "Dropper",
            Self::Hopper => "Hopper",
            Self::PlayerFloating => "Floating",
        }
    }

    // ContainerSetContent/ContainerOpen 用的窗口网络类型 null 表示不下发窗口
    pub const fn network_type(self) -> Option<u8> {
        match self {
            Self::Chest | Self::DoubleChest | Self::Player => Some(0),
            Self::Crafting | Self::Workbench => Some(1),
            Self::Furnace => Some(2),
            Self::EnchantTable => Some(3),
            Self::BrewingStand => Some(4),
            Self::Anvil => Some(5),
            Self::Dispenser => Some(6),
            Self::Dropper => Some(7),
            Self::Hopper => Some(8),
            Self::PlayerFloating => None,
        }
    }
}

// 配方材料 damage=None 等价于 PHP Item::get(id, null, 1) 任意 damage 都能用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ingredient {
    pub id: i16,
    pub damage: Option<i16>,
}

impl Ingredient {
    pub const fn any(id: i16) -> Self {
        Self { id, damage: None }
    }

    pub const fn exact(id: i16, damage: i16) -> Self {
        Self {
            id,
            damage: Some(damage),
        }
    }

    pub fn matches(&self, stack: &ItemStack) -> bool {
        !stack.is_empty()
            && stack.id == self.id
            && self.damage.is_none_or(|damage| stack.damage == damage)
    }

    // 配方表里通配 damage 按协议写 -1(0xFFFF) 与 Nukkit putSlot 一致
    // 客户端解析配方表支持这个值 但它绝不能进玩家背包
    pub fn to_stack(&self) -> ItemStack {
        ItemStack {
            id: self.id,
            count: 1,
            damage: self.damage.unwrap_or(-1),
        }
    }
}

// 有序配方 shape 每行 1..3 字符 空格表示空 同一字符可出现多次
#[derive(Debug, Clone)]
pub struct ShapedRecipe {
    pub result: ItemStack,
    width: usize,
    height: usize,
    grid: Vec<Option<Ingredient>>,
}

impl ShapedRecipe {
    pub fn new(result: ItemStack, shape: &[&str], symbols: &[(char, Ingredient)]) -> Self {
        let width = shape.iter().map(|row| row.chars().count()).max().unwrap_or(0);
        let height = shape.len();
        let mut grid = Vec::with_capacity(width * height);
        for row in shape {
            for x in 0..width {
                let key = row.chars().nth(x).unwrap_or(' ');
                let ingredient = if key == ' ' {
                    None
                } else {
                    symbols.iter().find(|(c, _)| *c == key).map(|(_, v)| *v)
                };
                grid.push(ingredient);
            }
        }
        Self {
            result,
            width,
            height,
            grid,
        }
    }

    // 从 recipes.json 的行优先格子表构造 id=0 为空 damage<0 为通配
    pub fn from_cells(
        result: ItemStack,
        width: usize,
        height: usize,
        cells: &[(i16, i16)],
    ) -> Self {
        let grid = cells
            .iter()
            .take(width * height)
            .map(|&(id, damage)| {
                (id > 0).then(|| {
                    if damage < 0 {
                        Ingredient::any(id)
                    } else {
                        Ingredient::exact(id, damage)
                    }
                })
            })
            .collect();
        Self {
            result,
            width,
            height,
            grid,
        }
    }

    // 供 CraftingDataPacket 编码 返回宽高与逐格材料(空格为 None)
    pub fn ingredient_map(&self) -> (usize, usize, Vec<Option<ItemStack>>) {
        let cells = self
            .grid
            .iter()
            .map(|cell| cell.map(|ingredient| ingredient.to_stack()))
            .collect();
        (self.width, self.height, cells)
    }

    // 输入是 3x3 合成网格 先裁掉全空行列 再按紧凑形状逐格比对
    // 左右镜像也允许 Minecraft 的工具配方可以放左侧或右侧
    pub fn matches(&self, input: &[Option<ItemStack>; 9]) -> bool {
        let Some((width, height, cells)) = crop_grid(input) else {
            return false;
        };
        if width != self.width || height != self.height {
            return false;
        }
        if self.cells_match(&cells) {
            return true;
        }
        let mut mirrored = Vec::with_capacity(cells.len());
        for y in 0..height {
            for x in (0..width).rev() {
                mirrored.push(cells[y * width + x]);
            }
        }
        self.cells_match(&mirrored)
    }

    fn cells_match(&self, cells: &[Option<ItemStack>]) -> bool {
        self.grid.iter().zip(cells).all(|(need, have)| match need {
            None => have.is_none_or(|stack| stack.is_empty()),
            Some(ingredient) => have.is_some_and(|stack| ingredient.matches(&stack)),
        })
    }
}

fn crop_grid(input: &[Option<ItemStack>; 9]) -> Option<(usize, usize, Vec<Option<ItemStack>>)> {
    let mut min_x = 3;
    let mut max_x = 0;
    let mut min_y = 3;
    let mut max_y = 0;
    let mut found = false;
    for y in 0..3 {
        for x in 0..3 {
            if input[y * 3 + x].is_some_and(|stack| !stack.is_empty()) {
                found = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if !found {
        return None;
    }
    let width = max_x - min_x + 1;
    let height = max_y - min_y + 1;
    let cells = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                input[(y + min_y) * 3 + x + min_x].filter(|stack| !stack.is_empty())
            })
        })
        .collect();
    Some((width, height, cells))
}

// 无序配方 材料按多重集合匹配 顺序任意
#[derive(Debug, Clone)]
pub struct ShapelessRecipe {
    pub result: ItemStack,
    pub ingredients: Vec<Ingredient>,
}

impl ShapelessRecipe {
    pub fn new(result: ItemStack, ingredients: &[Ingredient]) -> Self {
        Self {
            result,
            ingredients: ingredients.to_vec(),
        }
    }

    pub fn ingredient_stacks(&self) -> Vec<ItemStack> {
        self.ingredients.iter().map(|i| i.to_stack()).collect()
    }

    pub fn matches(&self, input: &[Option<ItemStack>; 9]) -> bool {
        let stacks: Vec<ItemStack> = input
            .iter()
            .flatten()
            .copied()
            .filter(|stack| !stack.is_empty())
            .collect();
        if stacks.len() != self.ingredients.len() {
            return false;
        }
        let mut used = vec![false; stacks.len()];
        match_all(&self.ingredients, &stacks, &mut used, 0)
    }
}

// 回溯匹配 通配 damage 时贪心会选错 所以逐个试
fn match_all(
    need: &[Ingredient],
    stacks: &[ItemStack],
    used: &mut [bool],
    index: usize,
) -> bool {
    if index == need.len() {
        return true;
    }
    for (i, stack) in stacks.iter().enumerate() {
        if used[i] || !need[index].matches(stack) {
            continue;
        }
        used[i] = true;
        if match_all(need, stacks, used, index + 1) {
            return true;
        }
        used[i] = false;
    }
    false
}

// 熔炉配方 输入 damage 可通配
#[derive(Debug, Clone, Copy)]
pub struct FurnaceRecipe {
    pub input: Ingredient,
    pub result: ItemStack,
}

impl FurnaceRecipe {
    pub const fn new(input: Ingredient, result: ItemStack) -> Self {
        Self { input, result }
    }
}

// 燃料时长 Fuel.php 的 $duration 表 单位 tick 20 tick = 1 秒
pub fn fuel_ticks(id: i16) -> Option<u32> {
    let ticks = match id {
        ids::COAL => 1600,
        ids::COAL_BLOCK => 16000,
        // 岩浆桶 0.14.3 Fuel.php 里是 20000 tick
        ids::BUCKET => 20000,
        ids::BLAZE_ROD => 2400,
        ids::SAPLING | ids::STICK => 100,
        ids::WOODEN_AXE
        | ids::WOODEN_PICKAXE
        | ids::WOODEN_SWORD
        | ids::WOODEN_SHOVEL
        | ids::WOODEN_HOE => 200,
        ids::TRUNK
        | ids::TRUNK2
        | ids::WOODEN_PLANKS
        | ids::WOODEN_PRESSURE_PLATE
        | ids::FENCE
        | ids::FENCE_GATE
        | ids::FENCE_GATE_SPRUCE
        | ids::FENCE_GATE_BIRCH
        | ids::FENCE_GATE_JUNGLE
        | ids::FENCE_GATE_ACACIA
        | ids::FENCE_GATE_DARK_OAK
        | ids::OAK_WOODEN_STAIRS
        | ids::SPRUCE_WOODEN_STAIRS
        | ids::BIRCH_WOODEN_STAIRS
        | ids::JUNGLE_WOODEN_STAIRS
        | ids::ACACIA_WOODEN_STAIRS
        | ids::DARK_OAK_WOODEN_STAIRS
        | ids::TRAPDOOR
        | ids::WORKBENCH
        | ids::NOTEBLOCK
        | ids::BOOKSHELF
        | ids::CHEST
        | ids::TRAPPED_CHEST
        | ids::DAYLIGHT_DETECTOR => 300,
        _ => return None,
    };
    Some(ticks)
}

// 通用槽位容器 照搬 BaseInventory 的 add/remove 语义
// add_item 先叠同类再填空槽 返回放不下的剩余
#[derive(Debug, Clone)]
pub struct Inventory {
    pub kind: InventoryType,
    pub max_stack_size: u8,
    slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    pub fn new(kind: InventoryType) -> Self {
        Self {
            kind,
            max_stack_size: MAX_STACK_SIZE,
            slots: vec![None; kind.size()],
        }
    }

    pub fn size(&self) -> usize {
        self.slots.len()
    }

    pub fn title(&self) -> &'static str {
        self.kind.title()
    }

    pub fn contents(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn set_contents(&mut self, items: Vec<Option<ItemStack>>) {
        self.slots = items;
        self.slots.resize(self.kind.size(), None);
        for slot in &mut self.slots {
            if slot.is_some_and(|stack| stack.is_empty()) {
                *slot = None;
            }
        }
    }

    pub fn get(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    pub fn set(&mut self, index: usize, stack: Option<ItemStack>) {
        if let Some(slot) = self.slots.get_mut(index) {
            *slot = stack.filter(|item| !item.is_empty());
        }
    }

    pub fn first_empty(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.is_none_or(|stack| stack.is_empty()))
    }

    pub fn contains(&self, stack: &ItemStack) -> bool {
        self.count_of(stack) >= stack.count
    }

    pub fn count_of(&self, stack: &ItemStack) -> u8 {
        self.slots
            .iter()
            .flatten()
            .filter(|item| item.id == stack.id && item.damage == stack.damage)
            .map(|item| item.count)
            .sum::<u8>()
            .min(u8::MAX)
    }

    pub fn can_add_item(&self, stack: &ItemStack) -> bool {
        let mut left = stack.count;
        for item in self.slots.iter().flatten() {
            if item.id == stack.id && item.damage == stack.damage {
                left = left.saturating_sub(self.max_stack_size.saturating_sub(item.count));
            }
            if left == 0 {
                return true;
            }
        }
        let free = self
            .slots
            .iter()
            .filter(|slot| slot.is_none_or(|item| item.is_empty()))
            .count() as u8;
        left <= free.saturating_mul(self.max_stack_size)
    }

    pub fn add_item(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
        if stack.is_empty() {
            return None;
        }
        for slot in self.slots.iter_mut() {
            let Some(item) = slot.as_mut() else { continue };
            if item.id == stack.id
                && item.damage == stack.damage
                && item.count < self.max_stack_size
            {
                let take = stack.count.min(self.max_stack_size - item.count);
                item.count += take;
                stack.count -= take;
                if stack.count == 0 {
                    return None;
                }
            }
        }
        for slot in self.slots.iter_mut() {
            let empty = match slot {
                None => true,
                Some(item) => item.is_empty(),
            };
            if empty {
                *slot = Some(stack);
                return None;
            }
        }
        Some(stack)
    }

    pub fn remove_item(&mut self, stack: &ItemStack) -> bool {
        if !self.contains(stack) {
            return false;
        }
        let mut left = stack.count;
        for index in 0..self.slots.len() {
            let matched = self.slots[index]
                .is_some_and(|item| item.id == stack.id && item.damage == stack.damage);
            if !matched {
                continue;
            }
            let take = {
                let item = self.slots[index].as_mut().unwrap();
                let take = left.min(item.count);
                item.count -= take;
                take
            };
            left -= take;
            if self.slots[index].is_some_and(|item| item.count == 0) {
                self.slots[index] = None;
            }
            if left == 0 {
                return true;
            }
        }
        false
    }

    pub fn clear(&mut self, index: usize) {
        self.set(index, None);
    }

    pub fn clear_all(&mut self) {
        self.slots.fill(None);
    }
}

// 酿造配方 (基础药水 + 材料) -> 产物 0.14 客户端内置 只供服务端酿造台查表
#[derive(Debug, Clone, Copy)]
pub struct BrewingRecipe {
    pub potion: Ingredient,
    pub ingredient: Ingredient,
    pub result: ItemStack,
}

// 配方表 合并 recipes.json 与 CraftingManager.php 手写分支两套全集
#[derive(Debug, Clone)]
pub struct CraftingManager {
    pub shaped: Vec<ShapedRecipe>,
    pub shapeless: Vec<ShapelessRecipe>,
    pub furnace: Vec<FurnaceRecipe>,
    pub brewing: Vec<BrewingRecipe>,
}

impl Default for CraftingManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CraftingManager {
    // 两套配方取并集: recipes.json(useJson=true) + 手写分支(0.14.3 默认 useJson=false)
    pub fn new() -> Self {
        use crate::item::recipes::{FURNACE, SHAPED, SHAPELESS};
        use crate::item::recipes_hand::{BREWING, EXTRA_FURNACE, EXTRA_SHAPED};

        let mut shaped = Vec::with_capacity(SHAPED.len() + EXTRA_SHAPED.len());
        for raw in SHAPED.iter().chain(EXTRA_SHAPED.iter()) {
            let (id, count, damage) = raw.result;
            let result = ItemStack { id, count, damage };
            shaped.push(ShapedRecipe::from_cells(
                result,
                raw.width as usize,
                raw.height as usize,
                raw.cells,
            ));
        }

        let mut shapeless = Vec::with_capacity(SHAPELESS.len());
        for raw in SHAPELESS {
            let (id, count, damage) = raw.result;
            let ingredients: Vec<Ingredient> = raw
                .ingredients
                .iter()
                .map(|&(id, damage)| {
                    if damage < 0 {
                        Ingredient::any(id)
                    } else {
                        Ingredient::exact(id, damage)
                    }
                })
                .collect();
            shapeless.push(ShapelessRecipe::new(ItemStack { id, count, damage }, &ingredients));
        }

        let mut furnace = Vec::with_capacity(FURNACE.len() + EXTRA_FURNACE.len());
        for raw in FURNACE.iter().chain(EXTRA_FURNACE.iter()) {
            let (id, count, damage) = raw.result;
            let (input_id, input_damage) = raw.input;
            let input = if input_damage < 0 {
                Ingredient::any(input_id)
            } else {
                Ingredient::exact(input_id, input_damage)
            };
            furnace.push(FurnaceRecipe::new(input, ItemStack { id, count, damage }));
        }

        let mut brewing = Vec::with_capacity(BREWING.len());
        for raw in BREWING {
            let (id, count, damage) = raw.result;
            let (potion_id, potion_damage) = raw.potion;
            let (ingredient_id, ingredient_damage) = raw.ingredient;
            brewing.push(BrewingRecipe {
                potion: Ingredient::exact(potion_id, potion_damage),
                ingredient: Ingredient::exact(ingredient_id, ingredient_damage),
                result: ItemStack { id, count, damage },
            });
        }

        Self {
            shaped,
            shapeless,
            furnace,
            brewing,
        }
    }

    // 按 CraftingData 下发的配方 UUID 反查 编号顺序与 container.rs 编码一致
    // 返回 (期望产物, 材料 id 列表) 用于校验客户端上报的合成事件
    pub fn recipe_by_uuid(&self, uuid: &[u8; 16]) -> Option<(ItemStack, Vec<Ingredient>)> {
        if !uuid.starts_with(b"ViolinRc") {
            return None;
        }
        let index = u64::from_be_bytes(uuid[8..].try_into().ok()?) as usize;
        if index < self.shapeless.len() {
            let r = &self.shapeless[index];
            return Some((r.result, r.ingredients.clone()));
        }
        let shaped_index = index - self.shapeless.len();
        let r = self.shaped.get(shaped_index)?;
        let ingredients = r
            .ingredient_map()
            .2
            .into_iter()
            .flatten()
            .map(|s| {
                if s.damage < 0 {
                    Ingredient::any(s.id)
                } else {
                    Ingredient::exact(s.id, s.damage)
                }
            })
            .collect();
        Some((r.result, ingredients))
    }

    // 酿造台查询 基础药水 + 材料 -> 产物
    pub fn brew(&self, potion: &ItemStack, ingredient: &ItemStack) -> Option<ItemStack> {
        self.brewing
            .iter()
            .find(|r| r.potion.matches(potion) && r.ingredient.matches(ingredient))
            .map(|r| r.result)
    }

    // 3x3 网格合成 先有序后无序 返回产物
    pub fn craft(&self, grid: &[Option<ItemStack>; 9]) -> Option<ItemStack> {
        for recipe in &self.shaped {
            if recipe.matches(grid) {
                return Some(recipe.result);
            }
        }
        for recipe in &self.shapeless {
            if recipe.matches(grid) {
                return Some(recipe.result);
            }
        }
        None
    }

    // 熔炉输入查产物 damage 通配的注册项放在精确项后面
    pub fn smelt(&self, input: &ItemStack) -> Option<ItemStack> {
        let mut wildcard = None;
        for recipe in &self.furnace {
            if !recipe.input.matches(input) {
                continue;
            }
            if recipe.input.damage.is_some() {
                return Some(recipe.result);
            }
            if wildcard.is_none() {
                wildcard = Some(recipe.result);
            }
        }
        wildcard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i16, count: u8) -> ItemStack {
        ItemStack { id, count, damage: 0 }
    }

    fn grid(cells: &[(usize, ItemStack)]) -> [Option<ItemStack>; 9] {
        let mut out = [None; 9];
        for (index, stack) in cells {
            out[*index] = Some(*stack);
        }
        out
    }

    #[test]
    fn inventory_type_matches_pocketmine_table() {
        assert_eq!(InventoryType::Chest.size(), 27);
        assert_eq!(InventoryType::DoubleChest.size(), 54);
        assert_eq!(InventoryType::Player.size(), 40);
        assert_eq!(InventoryType::Workbench.size(), 10);
        assert_eq!(InventoryType::Furnace.network_type(), Some(2));
        assert_eq!(InventoryType::PlayerFloating.network_type(), None);
    }

    #[test]
    fn inventory_stacks_then_uses_empty_slots() {
        let mut inv = Inventory::new(InventoryType::Chest);
        assert!(inv.add_item(item(ids::COBBLESTONE, 60)).is_none());
        assert!(inv.add_item(item(ids::COBBLESTONE, 10)).is_none());
        assert_eq!(inv.get(0), Some(item(ids::COBBLESTONE, 64)));
        assert_eq!(inv.get(1), Some(item(ids::COBBLESTONE, 6)));
        assert!(inv.remove_item(&item(ids::COBBLESTONE, 70)));
        assert_eq!(inv.size(), 27);
        assert!(inv.first_empty().is_some());
    }

    // 配方表来自 0.14.3 recipes.json 数量与内容对齐
    #[test]
    fn recipe_table_matches_genisys_json() {
        let manager = CraftingManager::new();
        // json 290/23/24 + 手写独有 11/0/3 + 酿造 109
        assert_eq!(manager.shaped.len(), 301);
        assert_eq!(manager.shapeless.len(), 23);
        assert_eq!(manager.furnace.len(), 27);
        assert_eq!(manager.brewing.len(), 109);
        // 木镐 第一行三块木板 中间和下面各一根木棍
        let pick = manager
            .shaped
            .iter()
            .find(|r| r.result.id == ids::WOODEN_PICKAXE)
            .unwrap();
        let (w, h, cells) = pick.ingredient_map();
        assert_eq!((w, h), (3, 3));
        assert_eq!(cells[0].map(|s| s.id), Some(ids::WOODEN_PLANKS));
        assert_eq!(cells[4].map(|s| s.id), Some(ids::STICK));
        assert_eq!(cells[7].map(|s| s.id), Some(ids::STICK));
        assert!(cells[1].is_none() == false && cells[3].is_none());
        // 通配 damage 的木板任意树种都能用
        assert!(Ingredient::any(ids::WOODEN_PLANKS).matches(&item(ids::WOODEN_PLANKS, 1)));
    }

    #[test]
    fn shaped_recipe_crops_and_mirrors() {
        let manager = CraftingManager::new();
        // 木棍在 0.14.3 里是 "X "/"X " 两行 裁掉空列后是 1x2
        let recipe = manager
            .shaped
            .iter()
            .find(|r| r.result.id == ids::STICK)
            .unwrap();
        let centered = grid(&[
            (1, item(ids::WOODEN_PLANKS, 1)),
            (4, item(ids::WOODEN_PLANKS, 1)),
        ]);
        assert!(recipe.matches(&centered));
        let wrong = grid(&[(0, item(ids::WOODEN_PLANKS, 1))]);
        assert!(!recipe.matches(&wrong));
    }

    #[test]
    fn shapeless_recipe_ignores_order() {
        let manager = CraftingManager::new();
        // 蘑菇汤: 任意棕蘑菇 + 任意红蘑菇 + 碗 顺序打乱仍能匹配
        let recipe = &manager.shapeless[0];
        let stacks: Vec<ItemStack> = recipe
            .ingredients
            .iter()
            .map(|i| {
                let mut s = i.to_stack();
                s.damage = 0;
                s
            })
            .collect();
        let mut cells = Vec::new();
        for (index, stack) in stacks.iter().enumerate() {
            cells.push((index, *stack));
        }
        assert!(recipe.matches(&grid(&cells)));
        // 打乱位置 结果一样
        let reversed: Vec<(usize, ItemStack)> = cells
            .iter()
            .rev()
            .enumerate()
            .map(|(i, (_, s))| (i, *s))
            .collect();
        assert!(recipe.matches(&grid(&reversed)));
        // 少一个材料就不能匹配
        assert!(!recipe.matches(&grid(&cells[..cells.len() - 1])));
    }

    #[test]
    fn brewing_table_resolves_potion() {
        let manager = CraftingManager::new();
        // 水瓶(373,0) + 地狱疣(372) -> 笨拙药水(373,4)
        let water = ItemStack { id: 373, count: 1, damage: 0 };
        let wart = ItemStack { id: 372, count: 1, damage: 0 };
        let result = manager.brew(&water, &wart).unwrap();
        assert_eq!((result.id, result.damage), (373, 4));
        assert!(manager.brew(&item(ids::STONE, 1), &wart).is_none());
    }

    #[test]
    fn furnace_and_fuel_tables() {
        let manager = CraftingManager::new();
        assert_eq!(manager.smelt(&item(ids::IRON_ORE, 1)), Some(item(ids::IRON_INGOT, 1)));
        assert_eq!(manager.smelt(&item(ids::SAND, 1)), Some(item(ids::GLASS, 1)));
        assert_eq!(manager.smelt(&item(ids::STONE, 1)), None);
        assert_eq!(fuel_ticks(ids::COAL), Some(1600));
        assert_eq!(fuel_ticks(ids::COAL_BLOCK), Some(16000));
        assert_eq!(fuel_ticks(ids::STICK), Some(100));
        assert_eq!(fuel_ticks(ids::BUCKET), Some(20000));
        assert_eq!(fuel_ticks(ids::STONE), None);
    }
}
