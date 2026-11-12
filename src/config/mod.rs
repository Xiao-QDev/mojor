// 配置入口 两份 toml:
//   server.toml 服务器基础配置 对标 Nukkit 的 server.properties
//   mojor.toml  高级配置 特性开关与性能调配
pub mod mojor;
pub mod server;

use std::ops::Deref;

pub use mojor::MoJorConfig;
pub use server::ServerConfig;

// 视距上限 再大单次要下发的区块数量会让内存与网络都吃不消
pub const MAX_VIEW_DISTANCE: i32 = 8;
// 0.14 客户端进服默认按 2 个区块半径要区块
pub const DEFAULT_VIEW_DISTANCE: i32 = 2;

// 组合成最终配置 基础项可直接通过 Deref 访问 config.bind / config.world
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub server: ServerConfig,
    pub mojor: MoJorConfig,
}

impl Deref for Config {
    type Target = ServerConfig;

    fn deref(&self) -> &ServerConfig {
        &self.server
    }
}

impl Config {
    // 先读两份 toml 缺文件就按内置模板生成一份 再让环境变量覆盖
    pub fn load() -> Self {
        let mut config = Self {
            // 先加载 mojor.toml 再 server.toml 与启动日志顺序一致
            mojor: load_or_create::<MoJorConfig>("mojor.toml", DEFAULT_MOJOR_TOML),
            server: load_or_create::<ServerConfig>("server.toml", DEFAULT_SERVER_TOML),
        };
        config.apply_env();
        config
    }

    // 兼容旧入口名
    pub fn from_env() -> Self {
        Self::load()
    }

    pub fn server_name(&self) -> String {
        self.server.server_name()
    }

    // 环境变量优先级最高 容器里不用改文件就能覆盖
    fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("MOJOR_BIND") {
            self.server.bind = v;
        }
        if let Ok(v) = std::env::var("MOJOR_MOTD") {
            self.server.motd = v;
        }
        if let Ok(v) = std::env::var("MOJOR_MAX_PLAYERS")
            && let Ok(n) = v.parse::<u32>()
        {
            self.server.max_players = n;
        }
        if let Ok(v) = std::env::var("MOJOR_GAMEMODE")
            && let Ok(n) = v.parse::<u8>()
        {
            self.server.gamemode = n & 0x01;
        }
        if let Ok(v) = std::env::var("MOJOR_WORLD") {
            self.server.world = v;
        }
        if let Ok(v) = std::env::var("MOJOR_GENERATOR") {
            self.server.generator = v;
        }
        if let Ok(v) = std::env::var("MOJOR_SEED")
            && let Ok(n) = v.parse::<u32>()
        {
            self.server.seed = n;
        }
        if let Ok(v) = std::env::var("MOJOR_VIEW_DISTANCE")
            && let Ok(n) = v.parse::<i32>()
        {
            self.server.view_distance = n.clamp(1, MAX_VIEW_DISTANCE);
        }
        if let Ok(v) = std::env::var("MOJOR_IO_WORKERS")
            && let Ok(n) = v.parse::<usize>()
        {
            self.mojor.io_workers = n;
        }
        if let Ok(v) = std::env::var("MOJOR_TOKIO_WORKERS")
            && let Ok(n) = v.parse::<usize>()
        {
            self.mojor.tokio_workers = n;
        }
        if let Ok(v) = std::env::var("MOJOR_LOG") {
            self.mojor.log_level = v;
        }
        // toml 里可能写到超范围 再夹一次
        self.server.view_distance = self.server.view_distance.clamp(1, MAX_VIEW_DISTANCE);
    }
}

// 配置模板随二进制一起编译进来 首次启动按模板生成带注释的文件
const DEFAULT_SERVER_TOML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/server.toml"));
const DEFAULT_MOJOR_TOML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/mojor.toml"));

// 文件不存在就按模板创建 然后读取解析 失败都回退默认值 不让坏配置挡住启动
fn load_or_create<T: serde::de::DeserializeOwned + Default>(path: &str, template: &str) -> T {
    tracing::info!("Loading {path}");
    if !std::path::Path::new(path).exists()
        && let Err(e) = std::fs::write(path, template)
    {
        tracing::warn!(path, error = %e, "配置文件创建失败 使用默认值");
    }
    match std::fs::read_to_string(path) {
        Ok(text) => match toml::from_str::<T>(&text) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(path, error = %e, "配置文件解析失败 使用默认值");
                T::default()
            }
        },
        Err(e) => {
            tracing::warn!(path, error = %e, "配置文件读取失败 使用默认值");
            T::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_name_uses_fixed_protocol() {
        let c = Config::default();
        assert_eq!(
            c.server_name(),
            format!("MCPE;{};70;{};0;20", c.motd, crate::config::server::VERSION)
        );
    }

    #[test]
    fn io_workers_zero_means_auto() {
        let mojor = MoJorConfig::default();
        assert!(mojor.effective_io_workers() >= 1);
    }

    // 文件不存在时要按模板创建出来 再读取解析
    #[test]
    fn creates_missing_config_file() {
        let dir = std::env::temp_dir().join(format!("mojor-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("server.toml");
        let _ = std::fs::remove_file(&path);
        let cfg: ServerConfig = load_or_create(path.to_str().unwrap(), "bind = \"1.2.3.4:1\"\n");
        assert_eq!(cfg.bind, "1.2.3.4:1");
        assert!(path.exists(), "配置文件应当被创建");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
