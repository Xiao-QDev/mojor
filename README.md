# mojor

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Protocol](https://img.shields.io/badge/MCPE-0.14.3%20(Protocol%2070)-green.svg)](https://minecraft.wiki/w/Minecraft_Wiki)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org/)

> A Minecraft: Pocket Edition (MCPE / Bedrock) server written from scratch in Rust.

**mojor** is a from-scratch MCPE server implementation targeting **MCPE 0.14.3 (protocol 70)**.
It does not depend on any third-party game library — instead it implements its own
RakNet transport, MCPE binary protocol, chunk storage format (`mojoregion`), and
noise-based terrain generation, all running on top of the `tokio` async runtime.

- **Protocol:** `CURRENT_PROTOCOL = 70` (MCPE 0.14.3)
- **Server version:** `mojor 0.0.1`
- **License:** [AGPLv3.0](LICENSE)

## Features

| Module | Description |
| --- | --- |
| **RakNet transport** | Custom reliable/ordered UDP transport: frame encapsulation, fragment reassembly, session management |
| **MCPE protocol** | Single-byte packet IDs for the 0.14 protocol: login, chunk sending, entity movement, world time, etc. |
| **World generation** | Noise-based terrain (default) + flat terrain (for testing); caves, ores, trees, vegetation population |
| **Chunk storage** | Custom `mojoregion` format (`.mjr`) with blob deduplication and region-file storage |
| **Chunk management** | View-distance driven load/unload, background prefetch thread pool, smooth dirty-block flush, periodic save |
| **Command system** | `/help`, `/list`, `/seed`, `/time`, `/version` |
| **Configuration** | Two TOML files (`server.toml` + `mojor.toml`) with environment-variable overrides |

## Repository layout

```text
mojor-main/
├── Cargo.toml              # Dependencies & package (edition 2024)
├── server.toml            # Basic server config (à la Nukkit's server.properties)
├── mojor.toml             # Advanced config: performance tuning & feature switches
├── src/
│   ├── main.rs            # Entry point: banner, logging, config load, task orchestration
│   ├── lib.rs             # Module declarations
│   ├── command/           # Command parsing & execution
│   ├── config/            # Config loading (server.toml / mojor.toml / env vars)
│   │   └── resources/banner.txt
│   ├── entity/            # Players, dropped items and other entities
│   ├── level/             # World, chunks, lighting, region management, generation
│   │   ├── generator/     # Noise terrain / flat / biomes / populators
│   │   └── format/        # Chunk serialization + mojoregion storage
│   ├── network/           # MCPE protocol parsing & networking
│   │   └── protocol/      # Per-packet (de)code (pid.rs defines packet IDs)
│   ├── raknet/            # RakNet server + protocol
│   ├── tick/              # Main tick-loop manager
│   └── utils/             # Binary read/write helpers
└── LICENSE                # AGPLv3.0
```

Build & run

Requires Rust (edition 2024 — a recent stable ≥ 1.85 or nightly) with a working cargo toolchain.

```bash
# Enter the source directory
cd mojor-main

# Build
cargo build --release

# Run
cargo run --release
```

On first run, server.toml, mojor.toml and resources/banner.txt are created
automatically, and an ASCII banner is printed (ANSI colors supported; auto-stripped
when not a terminal, when redirected, when NO_COLOR is set, or under Wine).

The server listens on 0.0.0.0:19132 (the standard MCPE UDP port) by default.
Connect with an MCPE 0.14.3 client at that address.

Configuration

server.toml (basic config, à la server.properties)

Field names use kebab-case; missing fields fall back to Default:

Field Default Description
bind 0.0.0.0:19132 Listen address host:port
motd Mojor Server For Minecraft:PE Server display name
max-players 20 Max player count
gamemode 1 0=survival, 1=creative
world world World directory (holds region/*.mjr)
generator terrain terrain noise / flat flat
seed 1337 World seed
view-distance 2 View distance in chunk radius, 1..=8
difficulty 1 Difficulty
pvp true Player-vs-player damage
spawn-protection 16 Spawn protection radius
auto-save true Auto save
white-list false Whitelist
enable-query true GameSpy4 query
enable-rcon false RCON (not yet wired in)

Note: some fields (mob spawning, whitelist, PvP, nether, RCON, etc.) are config
placeholders only — the gameplay logic is not fully wired in yet.

mojor.toml (advanced / performance)

Field Default Description
io-workers 0 Background chunk-load threads, 0=vCPU count
tokio-workers 0 tokio worker threads (via TOKIO_WORKER_THREADS)
unload-delay-ticks 40 Ticks to wait before unloading a chunk out of view
max-unload-per-tick 96 Max chunks unloaded per tick
max-flush-per-tick 16 Max dirty blocks flushed per tick
save-interval-ticks 200 Periodic save interval (ticks)
sync-interval-secs 12 Background disk-sync interval (seconds)
idle-shutdown-ticks 40 Ticks idle before a slice is reclaimed
prefetch-enabled true Enable load-circle prefetch
log-level mojor=info tracing filter, e.g. mojor=debug

Environment-variable overrides (highest priority, no file edits)

For containerized deployments, override via env vars: MOJOR_BIND, MOJOR_MOTD,
MOJOR_MAX_PLAYERS, MOJOR_GAMEMODE, MOJOR_WORLD, MOJOR_GENERATOR, MOJOR_SEED,
MOJOR_VIEW_DISTANCE, MOJOR_IO_WORKERS, MOJOR_TOKIO_WORKERS, MOJOR_LOG.
Additionally MOJOR_SERVER_ID pins the server ID (for stable RakNet handshakes).

Commands

The permission system is not implemented yet, so all online players may run these
(grammar aligns with Nukkit 0.14's TimeCommand):

Command Effect
/help, /? List available commands
/list List online players
/seed Show world seed
/time start Resume time progression
/time stop Pause time progression
/time query Query current time (ticks)
/time set <day\|night\|value> Set time, day=0, night=14000; negatives clamp to 0
/time add <value> Advance time
/version, /ver Show server version

Time constants match Nukkit: day=0, sunset=12000, night=14000, sunrise=23000, full=24000.

Architecture

Startup (src/main.rs)

1. Print the colored banner (before the logging system, no timestamp).
2. On Linux, lower the mmap threshold via mallopt + a 10s malloc_trim fallback to tame fragmentation.
3. Initialize tracing logging (custom [INFO] time prefix, Nukkit-style; reloaded to log-level after config load).
4. Load config (mojor.toml + server.toml + env vars).
5. Bind the UDP socket, build the RakNetServer listener (its own tokio task for the recv loop).
6. Enter tick::run: process network packets, tick the world (load/generate/unload chunks, entities, time), run commands.

Networking layers

· raknet/: RakNet over UDP — unreliable/reliable-ordered transport, frame encapsulation, connection sessions (server/session.rs, server/socket.rs, protocol/frame.rs, protocol/packets.rs).
· network/: MCPE protocol layer; protocol/pid.rs defines every single-byte packet ID, protocol/*.rs handle (de)code, network/mod.rs dispatches, network/login.rs / handlers.rs handle login & in-game packets.

World & chunks

· level/generator/: phase 1 generate_chunk (lay terrain), phase 2 populate_chunk (caves, ores, trees, vegetation) — the latter only for noise terrain.
· level/format/mojoregion/: custom region storage; blob.rs does blob dedup + compression (flate2), region_file.rs manages .mjr region files, store.rs is the backend.
· level/chunk_manager.rs + level/region_handler.rs: view-distance driven scheduling of chunk load/generate/unload/save, backed by background IO thread pools.

Protocol version

src/network/protocol/pid.rs pins the MCPE 0.14 single-byte packet-ID set, CURRENT_PROTOCOL = 70.
server.rs pins VERSION = "0.14.3".

Tests

The project ships unit tests (command parsing, config loading, protocol (de)code, etc.):

```bash
cargo test
```

License

Released under the AGPLv3.0 license. See the LICENSE file for details.
