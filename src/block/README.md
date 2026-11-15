# mojor Block 系统

本目录是从随附的 `block.zip`（PocketMine 0.14 / MCPE 协议 70 的方块类）完整移植到
Rust 的方块数据系统。PHP 侧 193 个类文件解析为 **180 个具体方块类 / 179 个唯一方块 ID**，
属性由原类自动提取并经继承链解析，无需手工维护。

## 文件

| 文件 | 内容 |
| --- | --- |
| `id.rs` | `BlockId`（=`u8`）类型别名 + 236 个具名 ID 常量（含历史别名），`CAMERA_U16` 单独给出 |
| `registry.rs` | `BlockInfo` 结构 + `BLOCKS: [Option<BlockInfo>; 256]` 静态注册表 + `ToolType` |
| `mod.rs` | `Block { id, meta }` 值对象、工厂、属性访问、查询函数与单元测试 |

## 提取的属性（对齐 PocketMine `Block` 类）

- `id` / `name`：方块 ID 与显示名称
- `name_map` + `name_mask`：按 4bit meta 变化的名称（如 6 种木头、16 种羊毛、7 种石头）
- `hardness`：硬度（负数=不可破坏，100=液体）
- `light`：发光等级 0..=15
- `solid` / `transparent`：固体 / 透明判定
- `light_filter`：光照穿过衰减（复刻 `Block::$lightFilter`，Lava=15、液体/冰=2 等）
- `friction`、`tool`（镐/斧/铲/剪）、`burn_chance` / `burn_ability`（可燃性）
- `can_be_activated` / `can_be_replaced` / `can_be_flowed_into` / `has_collision`

## 用法

```rust
use mojor::block::{self, Block, id, ToolType};

let stone = Block::from_id(id::STONE);
assert_eq!(stone.name(), "Stone");
assert!(stone.is_solid());
assert_eq!(stone.tool_type(), ToolType::Pickaxe);

// meta 变体名称
assert_eq!(Block::get(id::WOOD, 1).name(), "Spruce Wood");

// 查询函数
assert!(!block::is_solid(id::WATER));
assert_eq!(block::light_level(id::GLOWSTONE), 15);
assert_eq!(block::hardness(id::BEDROCK), -1.0);

// 枚举所有具名方块
for info in block::all_blocks() {
    println!("{} = {}", info.id, info.name);
}
assert_eq!(block::known_count(), 180);
```

## 与旧模块的关系

旧的 `level::generator::blocks` 保留原样，确保地形生成与既有测试零回归（217 个测试全过）。
新 `block` 模块是权威的完整数据源；两者对自然地形方块
（石头、草、泥土、水、树叶、矿石、冰、仙人掌等）的 solid/transparent 判定一致。
对人造/装饰方块（玻璃、门、火把、铁轨、红石等）新模块给出更完整的 PocketMine 语义。

## 验证

```bash
cargo test --lib block::   # 9 个方块测试
cargo test                 # 全套 217 个测试
cargo build --release
```
