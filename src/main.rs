use std::io::IsTerminal;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{Event, Subscriber, error, info};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, format::Writer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt, reload};

use mojor::config::Config;
use mojor::error::Result;
use mojor::level::ChunkLoader;
use mojor::level::generator::Generator;
use mojor::raknet::server::socket::RakNetServer;
use mojor::tick;

// 内置默认 banner 用户可以在运行目录的 resources/ 里放自己的文本覆盖
const DEFAULT_BANNER: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/config/resources/banner.txt"));

// 从运行目录的 resources 里找 banner 文本 支持 txt 与 ansi 取字典序第一个
// 目录为空就生成一份内置模板 用户照着改即可
fn load_banner() -> String {
    let dir = std::path::Path::new("resources");
    let _ = std::fs::create_dir_all(dir);
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_file()
                        && matches!(
                            path.extension().and_then(|ext| ext.to_str()),
                            Some("txt") | Some("ansi")
                        )
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    if let Some(path) = files.first()
        && let Ok(text) = std::fs::read_to_string(path)
    {
        return text;
    }
    let _ = std::fs::write(dir.join("banner.txt"), DEFAULT_BANNER);
    DEFAULT_BANNER.to_string()
}

// Wine 的 wineconsole 是 tty 但不支持 VT/ANSI
// 靠 ntdll 导出的 wine_get_version 识别 Wine 环境 只有 Wine 才有这个符号
#[cfg(windows)]
fn running_under_wine() -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleA(module: *const u8) -> *mut std::ffi::c_void;
        fn GetProcAddress(
            module: *mut std::ffi::c_void,
            name: *const u8,
        ) -> *mut std::ffi::c_void;
    }
    unsafe {
        let module = GetModuleHandleA(b"ntdll.dll\0".as_ptr());
        if module.is_null() {
            return false;
        }
        !GetProcAddress(module, b"wine_get_version\0".as_ptr()).is_null()
    }
}

#[cfg(not(windows))]
fn running_under_wine() -> bool {
    false
}

// 去掉 ANSI 转义序列 老终端或重定向到文件时输出纯文本
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            // 吃掉直到终止字母 如 m 或 h 或 l
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn print_banner() {
    let text = load_banner();
    // 非终端或设置了 NO_COLOR 时剥离颜色 老终端与日志文件都不会看到转义
    let text = if std::env::var_os("NO_COLOR").is_some()
        || !std::io::stdout().is_terminal()
        || running_under_wine()
    {
        strip_ansi(&text)
    } else {
        text
    };
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    let _ = std::io::Write::flush(&mut std::io::stdout());
}

// 启动日志用 [INFO] 前缀 对齐 Nukkit 的输出风格
struct BracketFormat;

impl<S, N> FormatEvent<S, N> for BracketFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        // 时间用 UTC 时分秒 服务器日志带个时间戳便于排查
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            % 86_400;
        write!(
            writer,
            "{:02}:{:02}:{:02} [{}] ",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60,
            event.metadata().level()
        )?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // 最开始直接打印彩色 banner 不等日志系统 也不带时间戳
    print_banner();

    #[cfg(target_os = "linux")]
    unsafe {
        libc::mallopt(libc::M_TRIM_THRESHOLD, 64 * 1024);
        libc::mallopt(libc::M_MMAP_THRESHOLD, 64 * 1024);
    }

    let started = std::time::Instant::now();

    // 先按环境变量/默认级别初始化 让配置文件加载过程也能打日志
    // 读取 mojor.toml 后再按 log-level reload 生效
    let (filter, reload_handle) = reload::Layer::new(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("mojor=info")),
    );
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().event_format(BracketFormat))
        .init();

    // 加载配置过程中会输出 Loading mojor.toml / server.toml
    let config = Arc::new(Config::load());
    let _ = reload_handle.reload(EnvFilter::new(config.mojor.log_level.as_str()));

    let mode = if config.gamemode & 1 == 0 {
        "SURVIVAL"
    } else {
        "CREATIVE"
    };
    info!("Default game type: {mode}");
    info!("Start Minecraft server on : {}", config.bind);
    info!(
        "This server is running Mojor version {} (MCPE: {}) (Plugin API {})",
        env!("CARGO_PKG_VERSION"),
        mojor::config::server::VERSION,
        env!("CARGO_PKG_VERSION")
    );
    info!("Mojor is released under the AGPLv3.0 license");
    info!("Preparing to generate the level \"{}\"", config.world);

    let socket = Arc::new(UdpSocket::bind(config.bind.as_str()).await?);
    let server_id = std::env::var("MOJOR_SERVER_ID")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0x006d_6f6a_6f72)
        });
    let (net_tx, net_rx) = mpsc::channel(1024);
    let (cmd_tx, cmd_rx) = mpsc::channel(256);

    let storage = ChunkLoader::open(&config.world)?;
    let generator = match config.generator.as_str() {
        "flat" => Generator::flat(),
        _ => Generator::terrain(config.seed),
    };

    let listener = RakNetServer::new(socket, &config, net_tx, server_id, cmd_rx);
    let mut udp = tokio::spawn(async move {
        if let Err(e) = listener.run().await {
            error!(error = %e, "UDP 收包循环异常退出");
        }
    });

    // 每 10 秒兜底 trim 一次 常规回收交给上面的 mallopt 阈值按需完成
    // 每秒扫堆在玩家多时会有毫秒级抖动 这里只做低频兜底
    #[cfg(target_os = "linux")]
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            // SAFETY: malloc_trim 是 glibc 的堆整理接口 无外部副作用
            unsafe {
                libc::malloc_trim(0);
            }
        }
    });

    info!(
        "Done ({:.4}s)! For help, type \"help\" or \"?\"",
        started.elapsed().as_secs_f64()
    );

    tokio::select! {
        res = tick::run(net_rx, cmd_tx, config.gamemode, storage, generator, config.seed, config.view_distance, config.mojor.effective_io_workers()) => { res?; }
        res = &mut udp => {
            if let Err(e) = res {
                error!(error = %e, "UDP task 异常结束");
            }
        }
    }

    udp.abort();
    info!("Server Closed");
    Ok(())
}
