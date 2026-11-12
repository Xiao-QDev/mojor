// server.toml 服务器基础配置 对标 Nukkit 的 server.properties
use serde::Deserialize;

use crate::network::protocol::pid::CURRENT_PROTOCOL;
use super::DEFAULT_VIEW_DISTANCE;

// 这个版本就是 0.14.3 不再走配置 改版本时只改这里
pub const VERSION: &str = "0.14.3";

// 键名统一 kebab-case 与 server.properties 风格一致 少写字段由 Default 兜底
// 部分字段(刷怪/白名单/PVP 等)是给玩法层预留的 目前尚未接入逻辑
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
#[allow(dead_code)]
pub struct ServerConfig {
    // 监听地址 host:port 对应 server-ip + server-port
    pub bind: String,
    pub motd: String,
    pub online_players: u32,
    pub max_players: u32,
    // 0 生存 1 创造
    pub gamemode: u8,
    // 世界目录 里面放 region 的 mjr 文件
    pub world: String,
    // 地形生成方式 flat 或 terrain
    pub generator: String,
    pub seed: u32,
    // 视距 单位是区块半径 1..=MAX_VIEW_DISTANCE
    pub view_distance: i32,

    // 以下对齐 server.properties 的基础项
    pub spawn_protection: u32,
    pub allow_flight: bool,
    pub spawn_animals: bool,
    pub spawn_mobs: bool,
    pub pvp: bool,
    pub difficulty: u8,
    pub level_type: String,
    pub generator_settings: String,
    pub auto_save: bool,
    pub white_list: bool,
    pub announce_player_achievements: bool,
    pub hardcore: bool,
    pub force_gamemode: bool,
    pub nether_enabled: bool,
    pub enable_query: bool,
    pub enable_rcon: bool,
    pub rcon_password: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:19132".into(),
            motd: "Mojor Server For Minecraft:PE".into(),
            online_players: 0,
            max_players: 20,
            gamemode: 1,
            world: "world".into(),
            generator: "terrain".into(),
            seed: 1337,
            view_distance: DEFAULT_VIEW_DISTANCE,
            spawn_protection: 16,
            allow_flight: false,
            spawn_animals: true,
            spawn_mobs: true,
            pvp: true,
            difficulty: 1,
            level_type: "DEFAULT".into(),
            generator_settings: String::new(),
            auto_save: true,
            white_list: false,
            announce_player_achievements: true,
            hardcore: false,
            force_gamemode: false,
            nether_enabled: false,
            enable_query: true,
            enable_rcon: false,
            rcon_password: String::new(),
        }
    }
}

impl ServerConfig {
    // MCPE 的 pong serverName 字段 分号分隔
    pub fn server_name(&self) -> String {
        format!(
            "MCPE;{};{};{};{};{}",
            self.motd, CURRENT_PROTOCOL, VERSION, self.online_players, self.max_players
        )
    }
}
