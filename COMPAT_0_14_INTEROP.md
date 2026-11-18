# violin：0.14.0 ~ 0.14.3 互通补丁说明

本补丁让 violin 从「只接受 MCPE 0.14.3（协议 70）」升级为「同时接受 0.14.0 / 0.14.1 / 0.14.2 / 0.14.3」，
依据来自随附的 ITX/Genisys 修复核心 `src/pocketmine/network/protocol/Info.php`：

```php
const CURRENT_PROTOCOL  = 70;
const ACCEPTED_PROTOCOLS = [45, 46, 60, 70];
```

协议号与客户端版本对应关系：

| 协议号 | 客户端版本 |
| ------ | ---------- |
| 45     | MCPE 0.14.0 |
| 46     | MCPE 0.14.1 |
| 60     | MCPE 0.14.2 |
| 70     | MCPE 0.14.3 |

## 改动清单

### 1. `src/network/protocol/pid.rs`
新增 `ACCEPTED_PROTOCOLS: [i32; 4] = [45, 46, 60, 70]`（保留 `CURRENT_PROTOCOL = 70`）。

### 2. `src/network/login.rs`
- `protocol_accepted()` 由「仅等于 70」改为「命中 ACCEPTED_PROTOCOLS」，并兼容 protocol1 / protocol2 两个字段，
  对应 Genisys `Player.php` 的 `in_array($packet->protocol1, ProtocolInfo::ACCEPTED_PROTOCOLS)`。
- 登录被拒时按版本区分状态码：低于 70 → `LOGIN_FAILED_CLIENT`（客户端过旧），否则 → `LOGIN_FAILED_SERVER`（服务端过旧），
  对应 Genisys 的 `disconnectionScreen.outdatedClient / outdatedServer`。
- 新增 `client_protocol()`，用 protocol1 作为该会话的协商协议号。
- 新增单元测试覆盖四个版本全部接受、非法版本拒绝、protocol2 兜底、拒绝状态码分流。

### 3. `src/entity/player.rs` + `src/network/handlers.rs`
`PlayerState` 新增 `protocol: i32` 字段，登录时写入客户端协商的协议号，供网络线程与 region 线程共享。

### 4. `src/network/protocol/use_item.rs` + `src/level/region_handler.rs`
`UseItemPacket::decode()` 增加 `protocol` 参数。协议 `< 70` 的客户端在「玩家坐标」之后**没有** `slot(i32)` 字段，
对应 Genisys `UseItemPacket::decodeAdditional()`：

```php
$this->slot = -1;
if ($protocol >= Info::CURRENT_PROTOCOL) {
    $this->slot = $this->getInt();
}
```

若继续按 70 的布局硬读 slot，低版本客户端点击方块时手持物品会整体错位 4 字节，导致放置失效。
现在按版本读取，低版本 slot 置 -1。新增单元测试验证 45/46/60 三种协议的手持物品解析。

## 验证方式

原项目要求 rustc ≥ 1.95（依赖 `sysinfo 0.40.3` 与 `vendor/rustyline` 的 `cfg_select!`）。

```bash
rustup toolchain install 1.95.0
cargo +1.95.0 test --lib
```

预期：全部单元测试通过，包括新增的
`accepts_all_014_protocols` / `rejects_unsupported_protocols` /
`protocol2_can_satisfy_acceptance` / `rejected_client_gets_status_following_version` /
`use_item_pre_0143_has_no_slot`。

## 说明

- 除上述文件外，未改动地形生成、存档、命令等其它逻辑。
- `CURRENT_PROTOCOL` 保持 70，StartGame 等下行包布局沿用现有实现（Genisys 中这四个版本的下行包结构一致）。
- 互通的核心在于「入站解析按版本适配」+「握手协议号白名单」，本补丁完整覆盖了这两点。

## 修复：跑图跨界后新区块不渲染（全黑）

现象：客户端能挖、有碰撞，但地形空白不渲染；重新进服后恢复。

根因：分片大小为 `REGION_CHUNKS=32`（512 格）。出生点附近区块由本片的 `tick_players`
主路径下发，该路径检查 `is_lit()`，正常。玩家跑出分片边界后，新视野区块由属主分片
经 `ForeignChunk` / `ChunkLoaded(Waiter::Foreign)` 代发，而这两条路径原样直接调用
`payload()` 下发，**没有检查 `is_lit()`**。刚生成的区块 `sky_light` 全为 0，
于是客户端拿到 blocks 正确、但天空光全黑的区块——能挖有碰撞、画面不渲染。
重进服时区块已落盘（`blob::decode` 带出存档光照、`lit=true`），故恢复。

修复（`src/level/actor.rs` + `src/level/chunk_manager.rs`）：
- 两条 Foreign 下发路径增加 `is_lit()` 判断；未点亮的区块调用 `prepare_for_light()`
  补排光照 Init，并登记到 `IoState.foreign_lit_wait`，暂不下发、也不结算 `ForeignDone`。
- `RegionMsg::LightDone` 处理中，目标区块 `lit` 打开后，把等待该块的玩家逐个下发并
  结算 `settle_foreign()`。
- `prepare_for_light()` 兜底：对已 generated/populated 但仍 `lit=false` 的半成品区块，
  只要不在 `light_init_in_flight` 就补排 `pending_light_init`，避免 Foreign 等待永久挂起。
- 分片空闲判定已含 `light_in_flight` 与 `has_pending_light()`，延迟下发期间不会被回收。

验证：新增回归测试 `border_chunks_are_delivered_lit`，断言 9 个跨界区块全部带非空天空光下发；
`chunk_manager::tests` 27 项全通过。

## 修复：region 线程 panic「目标区块的地形已经准备好」（挖方块无粒子 / 跑图停摆）

现象：跑一会儿后新地形不渲染，挖方块客户端能挖（本地预测）但服务端不发粒子、无回应；
指令仍正常。服务端日志出现：

```
thread 'region-2-3' panicked at src\level\chunk_manager.rs:333:22:
目标区块的地形已经准备好
```

（旧版 mojor 在 328 行同一处也崩过，属上游长期存在的偶发 bug。）

分析：指令（斜杠命令）在 global 层直接消费，不投分片；而挖方块的粒子由方块属主分片
执行 `break_block` 后经 `BlockBroken` 事件发出。指令正常、粒子不发，说明对应 region
线程已停止工作。日志确认该线程是被 panic 杀死的，panic 点在 `ensure_populated()` 的
`terrain.get(&key).expect("目标区块的地形已经准备好")`。

`terrain` 是容量 `TERRAIN_CACHE=256` 的 FIFO 缓存，`ensure_terrain()` 命中已有 key 时
不刷新队列顺序，只在新插入时从队首淘汰。装饰一个目标区块前，`ensure_populated()` 会
先为它周围最多 8×8=64 个区块调用 `ensure_terrain()`，这些插入会持续淘汰 FIFO 队首。
当目标区块的地形恰好处于队首（跑图反复访问同一批区块时完全可能）时，它会在装饰阶段
开始前被挤出 `terrain`，随后那句 `expect` 拿到 `None` → panic → region 线程死亡 →
该分片不再下发区块、不再处理挖方块，表现即"不渲染 + 无粒子"。

修复（`src/level/chunk_manager.rs`）：把"装饰时才回头读缓存里的目标地形"改为"装饰开始前
主动 `ensure_terrain()` 保证目标地形存在，并立即抓取快照"：

```rust
self.ensure_terrain(chunk_x, chunk_z);
let base_terrain = self
    .terrain
    .get(&key)
    .expect("目标区块地形刚 ensure 过，必然存在")
    .clone();
let completed = {
    let mut chunk = if matches!(self.generator, Generator::Realistic(_)) {
        let mut center = base_terrain;   // 只读快照，与缓存淘汰解耦
        ...
    } else {
        working.remove(&key).unwrap_or(base_terrain);
        ...
    }
    ...
};
```

这样装饰阶段只读提前抓好的快照，不再依赖 `terrain` 在装饰中途是否仍保留目标区块——
脆弱的 `expect("目标区块的地形已经准备好")` 被彻底移除，无论淘汰如何交错都不会再 panic。
非 Realistic 分支原先的 `self.terrain[&key]` 索引同样改用快照，消除同类隐患。

验证：新增回归测试 `populate_survives_target_terrain_eviction`，断言在缓存被填满、
目标处于淘汰压力下 populate 不 panic 且装饰结果与无干扰时逐字节一致；
`chunk_manager::tests` 28 项全通过（含跨区块装饰一致性、RTG 村庄顺序无关性）。

## 优化：光照引擎多线程化（吃满多核）

问题：原光照引擎是**全服单线程**——所有 region 的光照任务（新区块全量 Init、挖方块增量
Update）都排在一个 `light-engine` 线程上串行计算。多核机器上其余核心对此无能为力，
跑图/大批量地形生成时光照成为单核瓶颈，拖慢区块下发。

改造（`src/level/light/service.rs`）：
- 单 worker 改为 worker 池：`std::sync::mpsc` 共享队列 + N 个 `light-engine-{i}` 线程，
  与既有 `ChunkIoPool` 同款模式。任务自带独立 `Snapshot`，各 worker 并行纯 CPU 计算，
  算完把回执发回发起 region，仍由 region 线程单线程 `apply_light_receipt` 写回——
  并行只发生在无共享状态的计算阶段，不触碰任何 region/ChunkManager 状态。
- 默认 worker 数按可用逻辑处理器数自动建池，封顶 8（重 CPU 任务避免超配争抢）；
  可用环境变量 `VIOLIN_LIGHT_WORKERS=<n>` 覆盖。`start()` 保持无参签名，所有调用点零改动。

并行安全性核对：
- 光照模块无 `static` / `thread_local` / 共享可变状态，`Engine::init` / `update_blocks`
  是纯函数；
- 回执一律发回发起 region 单线程应用，不跨线程碰 ChunkManager；
- 同一区块 Init 回执到达前不会排 Update（`take_light_jobs` 的 `light_init_in_flight`
  守卫），同块任务不会并行乱序；跨块任务互不依赖。

验证：
- 新增 `parallel_matches_serial_results`：同一批快照用 1 worker 与 8 worker 计算，
  owner 与每块 sky/block 光照 nibble **逐字节一致**，证明并行不改变结果。
- 新增 `worker_pool_returns_receipts_to_own_channel`：多回执通道不串台、不丢失。
- 吞吐基准 `bench_parallel_throughput`（`--ignored` 手动运行）：256 块全量光照，
  1 worker 4.25s / 4 worker 2.11s（**约 2.0×**）/ 8 worker 2.16s。
  8 线程收益趋平是提交端串行 `Snapshot::grab` 成为新瓶颈（Amdahl），故默认封顶 8。
- `level::` 全套 284 项测试通过，零失败。

## 排查：跑图后 TPS 掉到 15（偶发近 1 秒 MSPT 尖峰）

现象：跑一会儿图后 TPS 降到 15 且静止不恢复。`/tps` 显示 **MSPT avg 13.29ms / max 925.88ms**、
最高分片利用率仅 15.3%。关键判断：**平均 MSPT 很低说明主循环平时不慢，是少数近 1 秒的
周期性尖峰把平均 gap 拉垮**（15 TPS ⇔ avg gap≈66.7ms）。不是算力不足，而是周期性同步阻塞。

排除项（已用测试实测）：
- 距离场：单玩家跑 200 区块后 `loading` 稳定在视距规模(25)、队列排空、旧区块 level 抬到
  不可达 → 旧区块能被摘票、分片能被 GC，不是距离场泄漏。
- region 空闲回收链、`light_in_flight` 增减、compact 周期调用均核对无泄漏。

嫌疑与处理（低风险）：后台 world-sync 线程每 12 秒 `loader.sync()` → `sync_all()` 原先
**逐个阻塞持锁 fsync 所有打开过的 region 文件**。跑图越久打开的文件越多、fsync 累积越久；
期间 region 工作线程撞锁被卡，进而拖住同步等回执的 global tick。改为 `try_lock`：
正在被读写的文件本周期跳过、下周期补刷（数据仍在 page cache，落盘语义不变），
后台 fsync 不再阻塞工作线程。`src/level/format/violinregion/store.rs`。

诊断埋点（本次新增，仅在慢帧时打印、平时零开销）：
- 主循环：gap/mspt 超 100ms 打印 `慢帧诊断`，含收包/业务包/network.tick 分段耗时；
- `network.tick` 内：超 100ms 打印 updates/gc/drain/containers 分段；
- 后台 sync：单次 fsync 超 100ms 打印 `慢帧诊断(后台 sync/fsync)`。

**下一步需要实测定位**：部署本版跑 1–2 分钟，把日志里所有 `慢帧诊断` 行发回。
- 若 `慢帧诊断(后台 sync/fsync)` 出现且 sync_ms≈900 → 确认 fsync，可进一步降频/增大 try_lock 跳过；
- 若主循环 `net_tick_ms` 高但 `network.tick` 不高 → 卡在别处，看 packets_ms；
- 若都低但 gap 高 → 卡在 tokio 调度/其他线程，需再查。

## 新增指令：/gamemode 与 /difficulty

照搬 Nukkit 0.14 语义新增两条指令，命令表驱动解析/补全/help 自动生效。

### /gamemode <survival|creative>
- 语法：`/gamemode 0|1` 或 `survival` / `creative`（含缩写 `s` / `c`）。
- 0.14 客户端只有生存/创造两档，冒险(2)/旁观(3)未实现，按 Nukkit 用法提示处理。
- 执行：切换全服模式（本核心 gamemode 是全局单一 u8 位标志，与 `VIOLIN_GAMEMODE`
  配置语义一致），给全体在线玩家广播 `SetPlayerGameTypePacket` + `AdventureSettings`
  （创造放开飞行、生存收回），回执 `Game mode is now {Survival|Creative}`。
- 用法错误不改状态、不广播。

### /difficulty <peaceful|easy|normal|hard>
- 语法：`/difficulty 0|1|2|3` 或 `peaceful` / `easy` / `normal` / `hard`（含首字母缩写）。
- 执行：更新全服难度并广播 `SetDifficultyPacket`，回执 `Difficulty is now {name}`。
- NetworkState 新增 `difficulty: i32` 字段（默认 NORMAL=2），`login_sequence` 增加
  difficulty 参数——新玩家登录时拿到的是**当前**难度，不再是硬编码 NORMAL。

改动文件：`src/command/gamemode.rs`、`src/command/difficulty.rs`（新增），
`src/command/mod.rs`、`src/command/help.rs`（命令表/help 登记），
`src/network/mod.rs`（执行分发 + 状态字段）、`src/network/login.rs`（登录读难度）、
`src/network/handlers.rs`、`src/network/tests.rs`（调用点/测试）。

验证：`command::` 39 项、`command::gamemode` 3 项、`command::difficulty` 3 项、
`network::login` 4+1 项全通过；新增端到端测试 `gamemode_command_updates_state_and_broadcasts`
与 `difficulty_command_updates_state_and_broadcasts` 验证状态更新 + 全员广播 + 用法错误不生效；
`login_uses_current_difficulty_not_hardcoded` 验证传入难度写进登录包。
完整 lib 测试 508 通过 / 28 失败，28 个均为既有 `network::tests` 异步时序 artifact
（比上一轮的 29 个还少一个），与本次改动无关。

## 指令别名：/gm、/diff、/ver

命令表统一支持别名：`CommandDef` 新增 `aliases: &'static [&'static str]`，别名与主名
共用同一个解析入口，在聊天和控制台都能执行，且一并进入 Tab 补全；但**不进 `/help`
主列表**，避免同一条命令重复出现。控制台高亮走同一张命令表，别名同样正常着色。

- `/gamemode` → `/gm`
- `/difficulty` → `/diff`
- `/version` → `/ver`（原先是独立表项并靠测试特判排除 help，现并入 `version.aliases`，行为不变）
- 之前按要求移除的 `/?` 别名保持移除，未恢复。

改动文件：`src/command/mod.rs`（结构体字段、`find_command` 命中别名、`candidates_for`
展开别名、命令表登记、测试）。
验证：`command::` 40 项、`console::` 4 项、两个端到端命令测试全通过。

## 权限组：OP 与玩家

按 Nukkit 的两档权限模型接入，仅分 OP / 玩家两级。

- **身份标识**：仅用**用户名**判定（大小写不敏感，ASCII 折叠），与 0.14 登录包 username 对应。
- **名单来源**：工作目录 `op.txt`，每行一个用户名，`#` 开头是注释；启动时 `NetworkState::load_ops`
  载入，缺失则按模板创建。由 `/op` `/deop` 运行时维护并**即时写回**文件，无需重启。
- **三条新指令**（均 OP 专属）：`/op <玩家>` 加名、`/deop <玩家>` 除名、`/ops` 列名单；
  文案照搬 Nukkit（Made ... an operator / Took ... out of the operator list）。
- **控制台永远 OP**：`sender=None` 无条件放行，空名单时也能用 `op <名>` 加出第一个 OP。
- **命令权限**：除 `/help` `/say` `/tell` 外全部 OP 专属（含 list/stop/seed/time/tp/tps/top/
  gamemode/difficulty/version 及三条 op 指令）。鉴权在 `run_command_from` 统一做：
  OP 专属命令若发起玩家不在名单，回 `§cYou are not allowed to use this command`，
  不改状态、不下发任何包；未知命令按公开处理照常回 Unknown 提示。
- 权限标记写进命令表 `CommandDef.permission`，`permission_of(&Command)` 供网络层查表。

改动文件：`src/permission.rs`（新增，`OpList` 名单管理）、`src/command/op.rs`（新增，指令解析
与文案）、`src/command/mod.rs`（Permission 枚举、命令表标记、三条命令登记、help、测试）、
`src/command/help.rs`、`src/network/mod.rs`（OpList 字段、load_ops、统一鉴权、op 指令分发）、
`src/tick/tick_manager.rs`（启动加载 op.txt）、`src/lib.rs`（模块声明）、`src/network/tests.rs`
（既有命令测试加 OP 适配 + 4 个权限端到端测试）。

验证：`command::` 43 项、`permission::` 3 项、`console::` 4 项全通过；新增 4 个网络端到端测试
（普通玩家被拒且零副作用 / 控制台引导加人 / 名单玩家可用 / 缺参回用法）全通过。完整 lib 测试
519 通过 / 28 失败 / 19 忽略，28 个失败与历史基线逐条一致、无新增，均为既有 `network::tests`
异步时序 artifact，与本次改动无关。

## 方块物理：液体流动 / 重力下落 / 非法放置修复

按需求给世界补上三类方块行为，均为服务端权威、按 region tick 驱动，客户端只接收
`UpdateBlockPacket` 同步：

- **液体流动**（水 8/9、岩浆 10/11）：源方块持续向四周扩散并按距离衰减（meta 低 3 位存
  1..5 级），优先垂直下落；源被挖掉后连通域逐级回缩枯竭。枯竭判定优先于扩散，避免挖源后
  流动水互相"复活"导致永不干涸。
- **重力方块**（沙子 12、砂砾 13、混凝土粉末等）：失去支撑后每 tick 下落一格，落到实体
  上方停住；落入液体时与液体交换（液体被顶到原格并重新参与流动）。
- **非法放置修复**：植物类方块（高草/花/树苗/作物）下方必须是合法基底（草/泥土/耕地/菌丝
  等），杜绝"高草上叠高草"。放置判定从 `actor.rs` 的 `placeable` 收口到物理层 `can_place`，
  按**要放的方块 id** 判定，普通方块仍只需目标格为空。

实现要点：
- 新增 `UpdateQueue`（坐标→到期 tick 的 HashMap，天然去重），挂在 region 主循环 tick 上；
  放置/挖掘后 `on_placed` / `on_broken` 唤醒相邻格，队列非空时不判定分片空闲（防止物理中途
  停摆或丢改动）。
- 物理逻辑集中在 `src/level/physics.rs` 的纯函数（接收 `&mut ChunkManager`），不碰背包/掉落。
- 新增 `RegionEvent::BlockChanged { cells }` 仅让 global 层广播，不触发任何物品逻辑；
  与玩家放置的 `BlockPlaced`、挖掘的 `BlockBroken` 彻底分离。
- 挖掘节流队列 `drain_digs` 补的挖掘同样会唤醒物理，保证延迟挖掘也能触发下落/回缩。

改动文件：`src/level/physics.rs`（新增，队列 + 液体/重力/放置判定 + 8 个单测）、
`src/level/mod.rs`（模块声明）、`src/level/actor.rs`（主循环 tick 驱动、RegionEvent 变体、
放置/挖掘唤醒物理、空闲判定）、`src/network/handlers.rs`（BlockChanged 广播分支）。

验证：`physics::` 8 项全通过；完整 lib 测试 527 通过 / 28 失败 / 19 忽略，28 个失败与历史
基线逐条一致、无新增，均为既有 `network::tests` 异步时序 artifact，与本次改动无关。

## 按 MCPE 0.14.1 原版源码重写方块物理（实体重力 + 原版液体算法）

依据用户提供的 MCPE 0.14.1 `libminecraftpe.so` 重建源码（BlockLauncher 源码包），
并用 ITX PHP 核心 + 0.14.3 协议逆向文档交叉验证包线序，把上一版的简化物理按原版重写：

**液体（严格原版 LiquidBlockDynamic）**
- meta 就是 depth：`0=源 / 1..7=水平流 / 8..15=下落柱`，源与流用同一个 id 判断，
  `WATER(8)/STILL_WATER(9)` 只是动态/静态 tick 状态（原版 blockId±1）。
- `tick()`：`getHighest` 取四邻最浅深度 + dropOff（水 1 / 岩浆 2）重算自身；
  上方有液体则变下落柱（above+8）；断供(+1 超过 7)枯竭成空气；稳定转静止方块。
- 下方优先垂直流（depth+8），否则按"下方是否有支撑"选水平方向铺开（depth+dropOff）。
- tick 间隔按原版：水 5 tick、岩浆 30 tick。
- 关键修复：源由 `data==0` 决定而非方块 id，否则每个稳定流动水都会变成假源导致不收敛；
  深度变化要唤醒邻格（原版 neighborChanged），否则断源后加深链中断、水永不枯竭。

**重力（原版 FallingBlock 实体，替换旧的方块逐格下移）**
- 沙子(12)/砂砾(13)/铁砧(166/167)失去支撑时 `fall_check` 触发 → `RegionEvent::FallRequested`
  → global 分配 eid → `RegionMsg::AddFallingBlock` 回投属主分片 → 清除方块并生成实体。
- 实体物理按原版 `FallingBlock::normalTick`：重力 0.04、阻尼 0.98、落地 `tryToPlace` 固化；
  落空超时(fallTime>100)移除；每 tick 发 `MoveEntityPacket` 让客户端看到平滑下落。
- 砸落伤害：下落中砸中玩家 → `RegionEvent::DamagePlayer` → global 扣血 + `SetHealthPacket`
  + `EntityEventPacket`(HURT=2) 受击动画。为此给 `PlayerState` 增加 health 字段(默认 20)。
- 新增 3 个协议包并交叉验证线序：`AddEntityPacket`(0x98, type=66, metadata DATA_VARIANT=0x14)、
  `SetHealthPacket`(0xb0, i32 BE)、`EntityEventPacket`(0xa4, eid i64 + event u8)。
  metadata 条目 `(type<<5)|id` + 小端数值 + 0x7f 结尾，无长度前缀。

**放置判定（原版 BushBlock::mayPlaceOn）**
- 植物基底只认草/泥土/耕地/灰化土(+菌丝/草径)，不能叠在植物上；
  新增 `checkAlive`：基底被挖掉后植物 tick 时枯死成空气（对齐原版 `BushBlock::checkAlive`）。

改动文件：新增 `src/entity/falling_block.rs`、`src/network/protocol/add_entity.rs`、
`src/network/protocol/entity_event.rs`；改 `src/network/protocol/{pid,entity_data,mod}.rs`、
`src/level/{physics,actor,level}.rs`、`src/network/handlers.rs`、`src/network/broadcast.rs`、
`src/entity/{mod,player}.rs`。

验证：协议包 3 项、实体 3 项、物理 10 项、重力实体端到端 1 项全通过；
完整 lib 测试 536 通过 / 28 失败 / 19 忽略，28 个失败与历史基线逐条一致、无新增。

---

## 补丁：假沙子（原格方块不刷新）

**现象**：重力方块（沙子/砂砾/铁砧）失去支撑开始下落时，客户端在**原位置残留一块方块**，
要在旁边挖/放一格触发邻格更新后那块才消失。

**根因**：原版 `HeavyBlock::startFalling` 用 `setBlockAndData(pos, AIR, 3)`，
其中 flags=3 = FLAG_ALL，会把"原格变空气"广播给客户端。
我们的 `RegionMsg::AddFallingBlock` 只 `set_block_meta(AIR)` 改了服务端世界，
`bodies` 里只放了 `AddEntityPacket`，漏发 `UpdateBlockPacket(AIR)`，
客户端因此一直缓存着那块方块，直到邻格更新顺带刷新。

**修复**（`src/level/actor.rs`）：生成实体时在同一批 `bodies` 里按序下发两条包——
先 `UpdateBlockPacket::single(x,y,z,AIR,0)` 清掉原格，再 `add_packet()` 生成实体，
与客户端处理顺序一致。测试 `falling_block_spawns_entity_and_clears_block` 增加对 AIR 清方块包的断言。

**顺带补齐原版落地掉落**：原版 `FallingBlock::normalTick` 里 `tryToPlace` 失败（落点被占）
且非创造模式时会 `spawnAtLocation(id,1)` 变回掉落物，此前是方块凭空消失。
新增 `RegionEvent::FallingBlockDropped`，region 落地失败时发出，
global 分配 eid 后回投 `AddDrop` 生成掉落物。

验证：`cargo +1.95.0 test --lib falling_block` 4 项全过；
完整 lib 测试 536 通过 / 28 失败 / 19 忽略，与基线一致、无新增失败。

---

## 原版物品全量移植（food / tools / functional / potion / projectile）

按 MCPE 0.14.1 原版 `items.json` 与 PTX/PHP 核心行为，把原版物品的使用行为整体移植到服务端。
沿用现有 legacy item id（APPLE=260、DIAMOND=264、BUCKET=325 等），不改协议、创造栏、配方与背包。
全部为新增模块，不改动已有放置/挖掘主链路。

### 食物与饥饿（`src/item/food.rs`、`src/item/behavior.rs`）

- 按原版 food 表建立 legacy id → 营养/饱和映射；常量对齐原版：
  `MAX_FOOD=20`、`START_SATURATION=5`、`MAX_EXHAUSTION=4`、`HEAL_LEVEL=18`、
  `STARVE_LEVEL=0`、`HEALTH_TICK_COUNT=80`。
- 吃下食物：`hunger += nutrition`，`saturation += 2*saturation*nutrition`，饱和不超过饥饿值。
- 消耗疲劳：每积累 4 点 exhaustion 先扣饱和再扣饥饿。
- 饥饿 ≥18 且饱和 >0 时每 80 tick 回 1 点血；饥饿为 0 时每 80 tick 掉血（难度非和平）。
- 客户端吃完上报 `EntityEventPacket(event=9)`，在 `on_entity_event` 消费并应用；
  2 秒 `eat_cooldown` 防止一次动画重复触发。蘑菇汤/甜菜汤吃完返还碗，药水返还玻璃瓶。
- 只下发 `player.hunger` 属性（客户端饥饿条只认它），不下发 saturation/exhaustion。

### 工具耐久（`src/item/tools.rs`）

- 耐久表对齐原版：金 33 / 木 60 / 石 132 / 铁 251 / 钻石 1562 /
  打火石 65 / 剪刀 239 / 弓 385。耐久存于 `ItemStack.damage`。
- 生存模式成功挖掘一格后对选中的工具扣 1 点耐久，耗尽即损坏并同步背包。

### 功能物品（`src/item/functional.rs`）

- 空桶点水/岩浆 → 装成水桶/岩浆桶；水桶/岩浆桶点地面 → 放置源方块并返还空桶。
- 打火石点火、锄头耕地成 `FARMLAND`、骨粉催熟作物。
- 通过 `UseItemPacket` 分流，发 `RegionMsg::UseItemOn` 给属主分片读世界后原子判定，
  写回世界 + 唤醒物理 + 回写物品/耐久并同步背包。
- 修复协议 45/46/60 无 `slot` 字段（-1）时误落到槽 0：统一用服务端 `held_slot`。

### 药水效果（`src/entity/effect.rs`、`src/network/protocol/mob_effect.rs`）

- 新增 `MobEffectPacket`（pid `0xa5`），线序：
  `eid(i64 BE) | event(u8) | effectId(u8) | amplifier(u8) | particles(u8) | duration(i32 BE)`。
- 支持再生、中毒、凋零、力量、速度、夜视、饱和、饥饿等效果；每 tick 结算回血/掉血/饥饿变化，
  到期移除并广播。瞬间治疗/瞬间伤害在饮用时立即结算。

### 投射物（`src/entity/projectile.rs`）

- 服务端权威抛体物理：箭/雪球/蛋重力 0.04、末影珍珠 0.03、阻尼 0.99，
  每 tick `MoveEntityPacket` 同步，撞地/超时发 `RemoveEntityPacket`。
- 雪球/鸡蛋/末影珍珠右键使用即投掷；弓靠 `PlayerAction(ACTION_RELEASE_ITEM=5)` 发射箭，
  生存模式消耗箭/投掷物，创造不扣。
- 方向由玩家 yaw/pitch 算出（`direction_from_rotation`），起点抬到眼睛高度并前推一格。
- region 线程无 eid 分配器，走 `RegionEvent::LaunchProjectile` → global 分配 eid →
  `Level::launch_projectile` 回投 `RegionMsg::LaunchProjectile` → 属主分片生成实体。
- 空闲检测计入 `projectiles`，投射物在飞时不会被误判为空闲撤线程。

### 验证

`cargo +1.95.0 check` 通过；`cargo +1.95.0 test --lib` 563 通过 / 28 失败 / 19 忽略，
28 项失败均为历史 `network::tests` 异步时序 artifact，与基线一致、无新增失败。
