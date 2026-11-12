// 游戏实体 与网络协议解耦 由 region 线程独占
// 这里只放状态与物理 收发包一律走 network 层
pub mod item_drop;
pub mod player;

pub(crate) use item_drop::ItemDrop;
pub(crate) use player::{
    PlayerDirectory, PlayerSnapshot, PlayerState, RegionPlayer,
};

// 玩家出生坐标 世界坐标常量
pub const SPAWN_POSITION: (f32, f32, f32) = (0.5, 5.0, 0.5);

// 掉落物碰撞盒半高 0.25 见方
pub const ITEM_HALF_HEIGHT: f32 = 0.125;
pub const ITEM_GRAVITY: f32 = 0.04;
pub const ITEM_MAX_FALL_SPEED: f32 = 4.0;
// 刚丢出来的前几 tick 不能被捡
pub const ITEM_PICKUP_DELAY: u32 = 10;
// 掉落物存活 tick 数
pub const ITEM_LIFETIME: u32 = 6000;

// 玩家碰撞盒 0.6 x 1.8 与 Nukkit Entity 一致
pub const PLAYER_HALF_WIDTH: f32 = 0.3;
pub const PLAYER_HEIGHT: f32 = 1.8;
// Nukkit 的拾取判定把玩家碰撞盒水平扩 1 格 垂直扩 0.5 格 再与物品碰撞盒求交
pub const ITEM_PICKUP_GROW_HORIZONTAL: f32 = 1.0;
pub const ITEM_PICKUP_GROW_VERTICAL: f32 = 0.5;
