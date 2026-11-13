# mojor

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Protocol](https://img.shields.io/badge/MCPE-0.14.3%20(Protocol%2070)-green.svg)](https://minecraft.wiki/w/Minecraft_Wiki)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org/)

> 一个从零开始用 Rust 编写的 Minecraft: Pocket Edition（MCPE / Bedrock）服务器。

**mojor** 是一个从零实现的 MCPE 服务器，目标版本为 **MCPE 0.14.3（协议 70）**。
它不依赖任何第三方游戏库——而是自行实现了 RakNet 传输层、MCPE 二进制协议、
区块存储格式（`mojoregion`）以及基于噪声的地形生成，全部运行在 `tokio`
异步运行时之上。

- **协议：** `CURRENT_PROTOCOL = 70`（MCPE 0.14.3）
- **服务器版本：** `mojor 0.0.1`
- **许可证：** [AGPLv3.0](LICENSE)

## 功能特性

| 模块 | 描述 |
| --- | --- |
| **RakNet 传输层** | 自定义的可靠/有序 UDP 传输：帧封装、分片重组、会话管理 |
| **MCPE 协议** | 0.14 协议的单字节包 ID：登录、区块发送、实体移动、世界时间等 |
| **世界生成** | 基于噪声的地形（默认）+ 平坦地形（用于测试）；洞穴、矿石、树木、植被填充 |
| **区块存储** | 自定义 `mojoregion` 格式（`.mjr`），带 blob 去重与区域文件存储 |
| **区块管理** | 视距驱动的加载/卸载、后台预取线程池、平滑脏方块刷新、定期保存 |
| **命令系统** | `/help`、`/list`、`/seed`、`/time`、`/version` |
| **配置** | 两个 TOML 文件（`server.toml` + `mojor.toml`），支持环境变量覆盖 |

## 仓库结构

```text
mojor-main/
├── Cargo.toml              # 依赖与包定义（edition 2024）
├── server.toml            # 基础服务器配置（类似 Nukkit 的 server.properties）
├── mojor.toml             # 高级配置：性能调优与功能开关
├── src/
│   ├── main.rs            # 入口：banner、日志、配置加载、任务编排
│   ├── lib.rs             # 模块声明
│   ├── command/           # 命令解析与执行
│   ├── config/            # 配置加载（server.toml / mojor.toml / 环境变量）
│   │   └── resources/banner.txt
│   ├── entity/            # 玩家、掉落物及其他实体
│   ├── level/             # 世界、区块、光照、区域管理、生成
│   │   ├── generator/     # 噪声地形 / 平坦 / 生物群系 / 填充器
│   │   └── format/        # 区块序列化 + mojoregion 存储
│   ├── network/           # MCPE 协议解析与网络
│   │   └── protocol/      # 逐包编解码（pid.rs 定义包 ID）
│   ├── raknet/            # RakNet 服务器 + 协议
│   ├── tick/              # 主 tick 循环管理器
│   └── utils/             # 二进制读写辅助
└── LICENSE                # AGPLv3.0
```

## 构建与运行

需要 **Rust（edition 2024 —— 较新的稳定版 ≥ 1.85 或 nightly）** 以及可用的 `cargo` 工具链。

```bash
# 进入源码目录
cd mojor-main

# 构建
cargo build --release

# 运行
cargo run --release
```

首次运行时，会自动创建 `server.toml`、`mojor.toml` 与 `resources/banner.txt`，
并打印 ASCII banner（支持 ANSI 颜色；在非终端、被重定向、设置了 `NO_COLOR`
或在 Wine 下运行时自动剥离颜色）。

服务器默认监听 **`0.0.0.0:19132`**（标准 MCPE UDP 端口）。
使用 MCPE 0.14.3 客户端连接该地址即可。

## 配置

### `server.toml`（基础配置，类似 `server.properties`）

字段名使用 kebab-case；缺失字段回退到 `Default`：

| 字段 | 默认值 | 描述 |
| --- | --- | --- |
| `bind` | `0.0.0.0:19132` | 监听地址 `host:port` |
| `motd` | `Mojor Server For Minecraft:PE` | 服务器显示名称 |
| `max-players` | `20` | 最大玩家数 |
| `gamemode` | `1` | `0`=生存，`1`=创造 |
| `world` | `world` | 世界目录（存放 `region/*.mjr`） |
| `generator` | `terrain` | `terrain` 噪声 / `flat` 平坦 |
| `seed` | `1337` | 世界种子 |
| `view-distance` | `2` | 视距（区块半径），`1..=8` |
| `difficulty` | `1` | 难度 |
| `pvp` | `true` | 玩家间伤害 |
| `spawn-protection` | `16` | 出生点保护半径 |
| `auto-save` | `true` | 自动保存 |
| `white-list` | `false` | 白名单 |
| `enable-query` | `true` | GameSpy4 查询 |
| `enable-rcon` | `false` | RCON（尚未接入） |

> 注意：部分字段（怪物生成、白名单、PvP、下界、RCON 等）仅为配置占位——
> 游戏逻辑尚未完全接入。

### `mojor.toml`（高级 / 性能）

| 字段 | 默认值 | 描述 |
| --- | --- | --- |
| `io-workers` | `0` | 后台区块加载线程数，`0`=vCPU 数量 |
| `tokio-workers` | `0` | tokio 工作线程数（通过 `TOKIO_WORKER_THREADS`） |
| `unload-delay-ticks` | `40` | 区块离开视距后等待多少 tick 再卸载 |
| `max-unload-per-tick` | `96` | 每 tick 最大卸载区块数 |
| `max-flush-per-tick` | `16` | 每 tick 最大刷新脏方块数 |
| `save-interval-ticks` | `200` | 定期保存间隔（tick） |
| `sync-interval-secs` | `12` | 后台磁盘同步间隔（秒） |
| `idle-shutdown-ticks` | `40` | 空闲多少 tick 后回收线程切片 |
| `prefetch-enabled` | `true` | 启用加载圆预取 |
| `log-level` | `mojor=info` | tracing 过滤器，例如 `mojor=debug` |

### 环境变量覆盖（最高优先级，无需改文件）

用于容器化部署时，可通过环境变量覆盖：`MOJOR_BIND`、`MOJOR_MOTD`、
`MOJOR_MAX_PLAYERS`、`MOJOR_GAMEMODE`、`MOJOR_WORLD`、`MOJOR_GENERATOR`、
`MOJOR_SEED`、`MOJOR_VIEW_DISTANCE`、`MOJOR_IO_WORKERS`、`MOJOR_TOKIO_WORKERS`、
`MOJOR_LOG`。此外 `MOJOR_SERVER_ID` 可固定服务器 ID（用于稳定的 RakNet 握手）。

## 命令

权限系统尚未实现，因此**所有在线玩家**都可以执行这些命令
（语法与 Nukkit 0.14 的 `TimeCommand` 一致）：

| 命令 | 效果 |
| --- | --- |
| `/help`、`/?` | 列出可用命令 |
| `/list` | 列出在线玩家 |
| `/seed` | 显示世界种子 |
| `/time start` | 恢复时间流逝 |
| `/time stop` | 暂停时间流逝 |
| `/time query` | 查询当前时间（tick） |
| `/time set <day\|night\|value>` | 设置时间，`day=0`、`night=14000`；负值钳制为 0 |
| `/time add <value>` | 推进时间 |
| `/version`、`/ver` | 显示服务器版本 |

时间常量与 Nukkit 一致：`day=0`、`sunset=12000`、`night=14000`、`sunrise=23000`、`full=24000`。

## 架构

### 启动流程（`src/main.rs`）

1. 打印彩色 banner（在日志系统之前，无时间戳）。
2. 在 Linux 上通过 `mallopt` 降低 mmap 阈值，并加 10s 的 `malloc_trim` 兜底以缓解内存碎片。
3. 初始化 `tracing` 日志（自定义 `[INFO]` 时间前缀，Nukkit 风格；配置加载后按 `log-level` 重载）。
4. 加载配置（`mojor.toml` + `server.toml` + 环境变量）。
5. 绑定 UDP socket，构建 `RakNetServer` 监听器（自己的 `tokio` 任务负责 recv 循环）。
6. 进入 `tick::run`：处理网络包、tick 世界（加载/生成/卸载区块、实体、时间）、执行命令。

### 网络分层

- `raknet/`：基于 UDP 的 RakNet——不可靠/可靠有序传输、帧封装、连接会话（`server/session.rs`、`server/socket.rs`、`protocol/frame.rs`、`protocol/packets.rs`）。
- `network/`：MCPE 协议层；`protocol/pid.rs` 定义所有单字节包 ID，`protocol/*.rs` 负责编解码，`network/mod.rs` 负责分发，`network/login.rs` / `handlers.rs` 处理登录与游戏内包。

### 世界与区块

- `level/generator/`：阶段一 `generate_chunk`（铺设地形），阶段二 `populate_chunk`（洞穴、矿石、树木、植被）——后者仅用于噪声地形。
- `level/format/mojoregion/`：自定义区域存储；`blob.rs` 负责 blob 去重 + 压缩（`flate2`），`region_file.rs` 管理 `.mjr` 区域文件，`store.rs` 是后端。
- `level/chunk_manager.rs` + `level/region_handler.rs`：视距驱动的区块加载/生成/卸载/保存调度，由后台 IO 线程池支撑。

### 协议版本

`src/network/protocol/pid.rs` 固定了 MCPE 0.14 的单字节包 ID 集合，`CURRENT_PROTOCOL = 70`。
`server.rs` 固定 `VERSION = "0.14.3"`。

## 测试

项目包含单元测试（命令解析、配置加载、协议编解码等）：

```bash
cargo test
```

## 许可证

以 **AGPLv3.0** 许可证发布。详情见 [LICENSE](LICENSE) 文件。