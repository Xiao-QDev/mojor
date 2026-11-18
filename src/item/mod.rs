// 物品层 照搬 Nukkit 的 item 子系统
// ids.rs: 物品 id 常量(含方块 id 与 Nukkit 同源)
// creative.rs: 创造背包内容(照搬 Item.initCreativeItems)
// inventory.rs: 背包容器与配方(照搬 example/inventory 的 PocketMine 实现)
// recipes.rs: recipes.json 配方表 recipes_hand.rs: CraftingManager.php 手写分支独有配方

pub mod behavior;
pub mod containers;
pub mod creative;
pub mod drops;
pub mod food;
pub mod functional;
pub mod ids;
pub mod tools;
pub mod inventory;
pub mod recipes;
pub mod recipes_hand;
