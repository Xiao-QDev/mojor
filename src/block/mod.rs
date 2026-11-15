//! mojor 方块系统：对齐 PocketMine 0.14（协议 70）的全部方块。
//!
//! 本模块是从随附的 `block.zip`（PocketMine 方块类）移植而来的方块数据模型，
//! 为 world 生成器提供统一的方块 ID、meta 语义与属性查询。设计要点：
//!
//! - 方块 ID 用 `u8`（0..=255），与区块存储和网络协议一致；meta 用 4bit。
//! - [`BlockId`] 是 `u8` 的别名，[`id`] 模块给出所有具名常量。
//! - [`Block`] 是 `(id, meta)` 的轻量值对象，提供 [`Block::get`] 工厂与属性访问。
//! - [`registry`] 静态表覆盖 180 个具名方块，属性由 PocketMine 类自动提取。
//! - 查询函数 [`is_solid`] / [`is_transparent`] / [`light_filter`] 等供光照与
//!   地形生成复用；语义严格对齐 PocketMine（比旧的简化判定更完整）。
//! - 旧的 `level::generator::blocks` 保留原样以确保地形生成零回归；本模块是
//!   权威的完整方块数据源，二者对自然地形方块（石头/草/泥土/水/树叶等）判定一致。

pub mod id;
pub mod registry;

use registry::{BlockInfo, ToolType, BLOCKS};

pub use id::BlockId;
pub use registry::{ToolType as BlockToolType, BLOCKS as BLOCK_REGISTRY};

/// 一个方块实例：ID + 4bit meta（对齐 PocketMine `Block` 的 id/meta）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub meta: u8,
}

impl Block {
    /// 由 id 与 meta 构造方块（meta 高 4 位被忽略）。
    pub fn get(id: BlockId, meta: u8) -> Self {
        Self { id, meta: meta & 0x0f }
    }

    /// 由 id 构造、meta=0 的方块。
    pub fn from_id(id: BlockId) -> Self {
        Self::get(id, 0)
    }

    /// 空气方块。
    pub fn air() -> Self {
        Self::from_id(id::AIR)
    }

    /// 该方块的静态属性；未定义 ID 返回 `None`。
    pub fn info(&self) -> Option<&'static BlockInfo> {
        info(self.id)
    }

    /// 显示名称（考虑 meta 变体，如不同木头/羊毛）。未定义 ID 返回 `"Unknown"`。
    pub fn name(&self) -> &'static str {
        self.info().map(|i| i.display_name(self.meta)).unwrap_or("Unknown")
    }

    pub fn hardness(&self) -> f32 {
        self.info().map(|i| i.hardness).unwrap_or(1.0)
    }

    pub fn light_level(&self) -> u8 {
        self.info().map(|i| i.light).unwrap_or(0)
    }

    pub fn is_solid(&self) -> bool {
        is_solid(self.id)
    }

    pub fn is_transparent(&self) -> bool {
        is_transparent(self.id)
    }

    pub fn light_filter(&self) -> u8 {
        light_filter(self.id)
    }

    pub fn tool_type(&self) -> ToolType {
        self.info().map(|i| i.tool).unwrap_or(ToolType::None)
    }

    pub fn can_be_activated(&self) -> bool {
        self.info().map(|i| i.can_be_activated).unwrap_or(false)
    }

    pub fn can_be_replaced(&self) -> bool {
        self.info().map(|i| i.can_be_replaced).unwrap_or(false)
    }

    pub fn can_be_flowed_into(&self) -> bool {
        self.info().map(|i| i.can_be_flowed_into).unwrap_or(false)
    }

    pub fn has_collision(&self) -> bool {
        self.info().map(|i| i.has_collision).unwrap_or(false)
    }

    pub fn friction(&self) -> f32 {
        self.info().map(|i| i.friction).unwrap_or(0.6)
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::air()
    }
}

/// 取方块静态属性；未定义 ID 返回 `None`。
pub fn info(id: BlockId) -> Option<&'static BlockInfo> {
    BLOCKS.get(id as usize).and_then(|o| o.as_ref())
}

/// 是否为固体方块（阻挡实体移动）。未知 ID 视为固体。
pub fn is_solid(id: BlockId) -> bool {
    info(id).map(|i| i.solid).unwrap_or(true)
}

/// 是否透明（阳光可透过）。未知 ID 视为不透明。
pub fn is_transparent(id: BlockId) -> bool {
    info(id).map(|i| i.transparent).unwrap_or(false)
}

/// 光照穿过该方块后的衰减值（见 PocketMine `Block::$lightFilter`）。
pub fn light_filter(id: BlockId) -> u8 {
    info(id).map(|i| i.light_filter).unwrap_or(1)
}

/// 方块硬度；未知 ID 返回 1.0。负数表示不可破坏。
pub fn hardness(id: BlockId) -> f32 {
    info(id).map(|i| i.hardness).unwrap_or(1.0)
}

/// 方块发光等级 0..=15；未知 ID 返回 0。
pub fn light_level(id: BlockId) -> u8 {
    info(id).map(|i| i.light).unwrap_or(0)
}

/// ID 是否为已注册的具名方块。
pub fn is_known(id: BlockId) -> bool {
    info(id).is_some()
}

/// 已注册具名方块的数量。
pub fn known_count() -> usize {
    BLOCKS.iter().filter(|o| o.is_some()).count()
}

/// 遍历所有已注册的具名方块（按 ID 升序）。
pub fn all_blocks() -> impl Iterator<Item = &'static BlockInfo> {
    BLOCKS.iter().filter_map(|o| o.as_ref())
}

/// 按默认名称精确查找方块 ID（大小写敏感）。
pub fn id_by_name(name: &str) -> Option<BlockId> {
    all_blocks().find(|i| i.name == name).map(|i| i.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_and_water_properties() {
        let stone = Block::from_id(id::STONE);
        assert_eq!(stone.name(), "Stone");
        assert!(stone.is_solid());
        assert!(!stone.is_transparent());
        assert_eq!(stone.hardness(), 1.5);
        assert_eq!(stone.tool_type(), ToolType::Pickaxe);

        assert!(!is_solid(id::WATER));
        assert!(is_transparent(id::WATER));
        assert_eq!(light_filter(id::STONE), 15);
        assert_eq!(light_filter(id::GLASS), 1);
    }

    #[test]
    fn meta_variant_names() {
        assert_eq!(Block::get(id::WOOD, 0).name(), "Oak Wood");
        assert_eq!(Block::get(id::WOOD, 1).name(), "Spruce Wood");
        assert_eq!(Block::get(id::STONE, 1).name(), "Granite");
        assert_eq!(Block::get(id::WOOL, 15).name(), "Black Wool");
        // meta 高 4 位被忽略
        assert_eq!(Block::get(id::WOOD, 0x15).name(), "Spruce Wood");
    }

    #[test]
    fn light_levels_and_unbreakable() {
        assert_eq!(light_level(id::GLOWSTONE), 15);
        assert_eq!(light_level(id::TORCH), 15);
        assert_eq!(light_level(id::STONE), 0);
        assert!(hardness(id::BEDROCK) < 0.0, " bedrock 不可破坏");
        assert!(hardness(id::LAVA) >= 100.0);
    }

    #[test]
    fn registries_are_consistent() {
        let mut known = 0;
        for id in 0u8..=255 {
            if is_known(id) {
                known += 1;
                // 每个具名方块都应能取到名称与属性
                assert!(!Block::from_id(id).name().is_empty());
            }
        }
        assert!(known >= 180, "应覆盖至少 180 个具名方块，实际 {known}");
    }

    #[test]
    fn liquids_and_doors() {
        assert!(Block::from_id(id::WATER).can_be_replaced());
        assert!(Block::from_id(id::LAVA).has_collision());
        assert!(Block::from_id(id::WOOD_DOOR_BLOCK).can_be_activated());
        assert!(!Block::from_id(id::WOOD_DOOR_BLOCK).is_solid());
    }

    #[test]
    fn enumeration_and_lookup() {
        assert_eq!(known_count(), all_blocks().count());
        assert_eq!(id_by_name("Stone"), Some(id::STONE));
        assert_eq!(id_by_name("Diamond Ore"), Some(id::DIAMOND_ORE));
        assert_eq!(id_by_name("no such block"), None);
        // 所有具名方块的 light_filter 应在 0..=15
        for b in all_blocks() {
            assert!(b.light_filter <= 15, "{} light_filter 越界", b.name);
        }
    }
}
