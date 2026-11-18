// MCPE 0.14 的包 ID 为固定单字节 无 VarInt
pub const LOGIN: u8 = 0x8f;
pub const PLAY_STATUS: u8 = 0x90;
pub const DISCONNECT: u8 = 0x91;
pub const BATCH: u8 = 0x92;
pub const TEXT: u8 = 0x93;
pub const SET_TIME: u8 = 0x94;
pub const START_GAME: u8 = 0x95;
pub const ADD_PLAYER: u8 = 0x96;
pub const REMOVE_PLAYER: u8 = 0x97;
// 通用实体包 原版 getId()=10 线上 PID=10-114=0x98 下落方块实体用
// 0x98 在原版与 PHP 核心都是 ADD_ENTITY 服务端此前空缺
pub const ADD_ENTITY: u8 = 0x98;
pub const REMOVE_ENTITY: u8 = 0x99;
pub const ADD_ITEM_ENTITY: u8 = 0x9a;
pub const TAKE_ITEM_ENTITY: u8 = 0x9b;
pub const MOVE_ENTITY: u8 = 0x9c;
pub const MOVE_PLAYER: u8 = 0x9d;
pub const REMOVE_BLOCK: u8 = 0x9e;
pub const UPDATE_BLOCK: u8 = 0x9f;
pub const LEVEL_EVENT: u8 = 0xa2;
// 实体事件 原版 getId()=22 线上 0xa4 砸落伤害动画(event=2)用
pub const ENTITY_EVENT: u8 = 0xa4;
pub const MOB_EFFECT: u8 = 0xa5;
pub const UPDATE_ATTRIBUTES: u8 = 0xa6;
pub const USE_ITEM: u8 = 0xaa;
pub const PLAYER_ACTION: u8 = 0xab;
pub const SET_ENTITY_DATA: u8 = 0xad;
pub const ANIMATE: u8 = 0xb2;
// 设置血量 原版 getId()=34 线上 0xb0 下落方块砸伤玩家用
pub const SET_HEALTH: u8 = 0xb0;
pub const SET_SPAWN_POSITION: u8 = 0xb1;
pub const RESPAWN: u8 = 0xb3;
pub const ADVENTURE_SETTINGS: u8 = 0xbc;
pub const FULL_CHUNK_DATA: u8 = 0xbf;
pub const SET_DIFFICULTY: u8 = 0xc0;
pub const SET_PLAYER_GAMETYPE: u8 = 0xc2;
pub const CONTAINER_SET_CONTENT: u8 = 0xb9;
pub const CONTAINER_OPEN: u8 = 0xb5;
pub const CONTAINER_CLOSE: u8 = 0xb6;
pub const CONTAINER_SET_SLOT: u8 = 0xb7;
pub const CONTAINER_SET_DATA: u8 = 0xb8;
pub const CRAFTING_DATA: u8 = 0xba;
pub const CRAFTING_EVENT: u8 = 0xbb;
pub const PLAYER_LIST: u8 = 0xc3;
pub const REQUEST_CHUNK_RADIUS: u8 = 0xc8;
pub const CHUNK_RADIUS_UPDATE: u8 = 0xc9;

// 当前实现的目标协议 0.14.3
pub const CURRENT_PROTOCOL: i32 = 70;

// MCPE 0.14 各小版本协议号 与 ITX/Genisys 的 Info.php::ACCEPTED_PROTOCOLS 对齐
// 45 = 0.14.0  46 = 0.14.1  60 = 0.14.2  70 = 0.14.3
// 客户端 protocol1/protocol2 命中其中一个即允许进入 实现 0.14.0-0.14.3 互通
pub const ACCEPTED_PROTOCOLS: [i32; 4] = [45, 46, 60, 70];
