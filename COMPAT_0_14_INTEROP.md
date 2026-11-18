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
