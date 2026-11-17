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
