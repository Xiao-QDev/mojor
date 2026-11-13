# mojor

[English](README.md) | [简体中文](README.zh-CN.md) | [日本語](README.ja.md)

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)
[![Protocol](https://img.shields.io/badge/MCPE-0.14.3%20(Protocol%2070)-green.svg)](https://minecraft.wiki/w/Minecraft_Wiki)
[![Rust](https://img.shields.io/badge/rust-edition%202024-orange.svg)](https://www.rust-lang.org/)

> Rust でゼロから書かれた Minecraft: Pocket Edition（MCPE / Bedrock）サーバー。

**mojor** は **MCPE 0.14.3（プロトコル 70）** を対象とした、ゼロから実装された
MCPE サーバーです。サードパーティ製のゲームライブラリには一切依存せず、RakNet
トランスポート、MCPE バイナリプロトコル、チャンク保存フォーマット（`mojoregion`）、
ノイズベースの地形生成をすべて独自実装し、`tokio` 非同期ランタイム上で動作します。

- **プロトコル：** `CURRENT_PROTOCOL = 70`（MCPE 0.14.3）
- **サーバーバージョン：** `mojor 0.0.1`
- **ライセンス：** [AGPLv3.0](LICENSE)

## 機能

| モジュール | 説明 |
| --- | --- |
| **RakNet トランスポート** | 独自の信頼性/順序保証付き UDP トランスポート：フレームカプセル化、フラグメント再結合、セッション管理 |
| **MCPE プロトコル** | 0.14 プロトコルの単一バイトパケット ID：ログイン、チャンク送信、エンティティ移動、ワールド時間など |
| **ワールド生成** | ノイズベース地形（デフォルト）+ フラット地形（テスト用）。洞窟、鉱石、樹木、植生のポピュレート |
| **チャンク保存** | 独自 `mojoregion` フォーマット（`.mjr`）。blob 重複排除とリージョンファイル保存 |
| **チャンク管理** | 視距離に基づくロード/アンロード、バックグラウンドプリフェッチスレッドプール、ダーティブロックのスムーズなフラッシュ、定期保存 |
| **コマンドシステム** | `/help`、`/list`、`/seed`、`/time`、`/version` |
| **設定** | 2 つの TOML ファイル（`server.toml` + `mojor.toml`）。環境変数による上書きに対応 |

## リポジトリ構成

```text
mojor-main/
├── Cargo.toml              # 依存関係とパッケージ（edition 2024）
├── server.toml            # 基本サーバー設定（Nukkit の server.properties 相当）
├── mojor.toml             # 詳細設定：パフォーマンスチューニングと機能スイッチ
├── src/
│   ├── main.rs            # エントリポイント：バナー、ログ、設定読み込み、タスク編成
│   ├── lib.rs             # モジュール宣言
│   ├── command/           # コマンドの解析と実行
│   ├── config/            # 設定読み込み（server.toml / mojor.toml / 環境変数）
│   │   └── resources/banner.txt
│   ├── entity/            # プレイヤー、ドロップアイテムなどのエンティティ
│   ├── level/             # ワールド、チャンク、ライティング、リージョン管理、生成
│   │   ├── generator/     # ノイズ地形 / フラット / バイオーム / ポピュレータ
│   │   └── format/        # チャンクシリアライズ + mojoregion 保存
│   ├── network/           # MCPE プロトコル解析とネットワーク
│   │   └── protocol/      # パケットごとのエンコード/デコード（pid.rs がパケット ID を定義）
│   ├── raknet/            # RakNet サーバー + プロトコル
│   ├── tick/              # メイン tick ループマネージャ
│   └── utils/             # バイナリ読み書きヘルパー
└── LICENSE                # AGPLv3.0
```

## ビルドと実行

**Rust（edition 2024 — 最近の安定版 ≥ 1.85 または nightly）** と動作する `cargo`
ツールチェーンが必要です。

```bash
# ソースディレクトリへ移動
cd mojor-main

# ビルド
cargo build --release

# 実行
cargo run --release
```

初回実行時に `server.toml`、`mojor.toml`、`resources/banner.txt` が自動生成され、
ASCII バナーが表示されます（ANSI カラー対応。非ターミナル、リダイレクト時、
`NO_COLOR` 設定時、Wine 上では自動的に色を除去）。

サーバーはデフォルトで **`0.0.0.0:19132`**（標準 MCPE UDP ポート）で待ち受けます。
MCPE 0.14.3 クライアントからそのアドレスに接続してください。

## 設定

### `server.toml`（基本設定、`server.properties` 相当）

フィールド名は kebab-case。欠落フィールドは `Default` にフォールバック：

| フィールド | デフォルト | 説明 |
| --- | --- | --- |
| `bind` | `0.0.0.0:19132` | 待ち受けアドレス `host:port` |
| `motd` | `Mojor Server For Minecraft:PE` | サーバー表示名 |
| `max-players` | `20` | 最大プレイヤー数 |
| `gamemode` | `1` | `0`=サバイバル、`1`=クリエイティブ |
| `world` | `world` | ワールドディレクトリ（`region/*.mjr` を保持） |
| `generator` | `terrain` | `terrain` ノイズ / `flat` フラット |
| `seed` | `1337` | ワールドシード |
| `view-distance` | `2` | 視距離（チャンク半径）、`1..=8` |
| `difficulty` | `1` | 難易度 |
| `pvp` | `true` | プレイヤー間ダメージ |
| `spawn-protection` | `16` | スポーン保護半径 |
| `auto-save` | `true` | 自動保存 |
| `white-list` | `false` | ホワイトリスト |
| `enable-query` | `true` | GameSpy4 クエリ |
| `enable-rcon` | `false` | RCON（未接続） |

> 注：一部のフィールド（モブスポーン、ホワイトリスト、PvP、ネザー、RCON など）は
> 設定上のプレースホルダのみで、ゲームプレイロジックはまだ完全には接続されていません。

### `mojor.toml`（詳細 / パフォーマンス）

| フィールド | デフォルト | 説明 |
| --- | --- | --- |
| `io-workers` | `0` | バックグラウンドのチャンクロードスレッド数、`0`=vCPU 数 |
| `tokio-workers` | `0` | tokio ワーカースレッド数（`TOKIO_WORKER_THREADS` 経由） |
| `unload-delay-ticks` | `40` | 視距離外になったチャンクをアンロードするまでの tick 数 |
| `max-unload-per-tick` | `96` | 1 tick あたりの最大アンロードチャンク数 |
| `max-flush-per-tick` | `16` | 1 tick あたりの最大ダーティブロックフラッシュ数 |
| `save-interval-ticks` | `200` | 定期保存間隔（tick） |
| `sync-interval-secs` | `12` | バックグラウンドのディスク同期間隔（秒） |
| `idle-shutdown-ticks` | `40` | スライス回収までのアイドル tick 数 |
| `prefetch-enabled` | `true` | ロードサークルのプリフェッチを有効化 |
| `log-level` | `mojor=info` | tracing フィルタ、例：`mojor=debug` |

### 環境変数による上書き（最優先、ファイル編集不要）

コンテナ運用向けに、環境変数で上書き可能：`MOJOR_BIND`、`MOJOR_MOTD`、
`MOJOR_MAX_PLAYERS`、`MOJOR_GAMEMODE`、`MOJOR_WORLD`、`MOJOR_GENERATOR`、
`MOJOR_SEED`、`MOJOR_VIEW_DISTANCE`、`MOJOR_IO_WORKERS`、`MOJOR_TOKIO_WORKERS`、
`MOJOR_LOG`。さらに `MOJOR_SERVER_ID` でサーバー ID を固定できます（安定した
RakNet ハンドシェイク用）。

## コマンド

権限システムは未実装のため、**すべてのオンラインプレイヤー**がこれらのコマンドを
実行できます（文法は Nukkit 0.14 の `TimeCommand` に準拠）：

| コマンド | 効果 |
| --- | --- |
| `/help`、`/?` | 利用可能なコマンド一覧 |
| `/list` | オンラインプレイヤー一覧 |
| `/seed` | ワールドシードを表示 |
| `/time start` | 時間の進行を再開 |
| `/time stop` | 時間の進行を停止 |
| `/time query` | 現在の時間（tick）を照会 |
| `/time set <day\|night\|value>` | 時間を設定、`day=0`、`night=14000`。負値は 0 にクランプ |
| `/time add <value>` | 時間を進める |
| `/version`、`/ver` | サーバーバージョンを表示 |

時間定数は Nukkit と一致：`day=0`、`sunset=12000`、`night=14000`、`sunrise=23000`、`full=24000`。

## アーキテクチャ

### 起動（`src/main.rs`）

1. カラーバナーを表示（ログシステムより前、タイムスタンプなし）。
2. Linux では `mallopt` で mmap 閾値を下げ、10 秒の `malloc_trim` フォールバックで断片化を抑制。
3. `tracing` ログを初期化（独自の `[INFO]` 時間プレフィックス、Nukkit スタイル。設定読み込み後に `log-level` で再読み込み）。
4. 設定を読み込み（`mojor.toml` + `server.toml` + 環境変数）。
5. UDP ソケットをバインドし、`RakNetServer` リスナーを構築（recv ループ用の専用 `tokio` タスク）。
6. `tick::run` へ：ネットワークパケット処理、ワールド tick（チャンクのロード/生成/アンロード、エンティティ、時間）、コマンド実行。

### ネットワーク層

- `raknet/`：UDP 上の RakNet——非信頼/信頼順序保証トランスポート、フレームカプセル化、接続セッション（`server/session.rs`、`server/socket.rs`、`protocol/frame.rs`、`protocol/packets.rs`）。
- `network/`：MCPE プロトコル層。`protocol/pid.rs` がすべての単一バイトパケット ID を定義し、`protocol/*.rs` がエンコード/デコードを担当、`network/mod.rs` がディスパッチ、`network/login.rs` / `handlers.rs` がログインとゲーム内パケットを処理。

### ワールドとチャンク

- `level/generator/`：フェーズ 1 `generate_chunk`（地形を敷設）、フェーズ 2 `populate_chunk`（洞窟、鉱石、樹木、植生）——後者はノイズ地形のみ。
- `level/format/mojoregion/`：独自リージョン保存。`blob.rs` が blob 重複排除 + 圧縮（`flate2`）、`region_file.rs` が `.mjr` リージョンファイルを管理、`store.rs` がバックエンド。
- `level/chunk_manager.rs` + `level/region_handler.rs`：視距離に基づくチャンクのロード/生成/アンロード/保存のスケジューリング。バックグラウンド IO スレッドプールが支える。

### プロトコルバージョン

`src/network/protocol/pid.rs` が MCPE 0.14 の単一バイトパケット ID セットを固定し、`CURRENT_PROTOCOL = 70`。
`server.rs` が `VERSION = "0.14.3"` を固定。

## テスト

プロジェクトにはユニットテスト（コマンド解析、設定読み込み、プロトコルのエンコード/デコードなど）が含まれます：

```bash
cargo test
```

## ライセンス

**AGPLv3.0** ライセンスで公開されています。詳細は [LICENSE](LICENSE) ファイルを参照してください。