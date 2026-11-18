// 工具耐久 照搬 ITX/原版 Tool::getMaxDurability 与 useOn
// 耐久存在物品 damage 字段: damage=0 全新, damage 递增到 max 时损坏消失
// 原版 ITX: 金 33 木 60 石 132 铁 251 钻石 1562 打火石 65 剪刀 239 弓 385
use super::ids;

// 返回该物品的最大耐久 None 表示不是耐久物品
pub fn max_durability(id: i16) -> Option<i32> {
    let d = match id {
        // 金
        ids::GOLD_PICKAXE | ids::GOLD_AXE | ids::GOLD_SHOVEL | ids::GOLD_HOE | ids::GOLD_SWORD => 33,
        // 木
        ids::WOODEN_PICKAXE | ids::WOODEN_AXE | ids::WOODEN_SHOVEL | ids::WOODEN_HOE
        | ids::WOODEN_SWORD => 60,
        // 石
        ids::STONE_PICKAXE | ids::STONE_AXE | ids::STONE_SHOVEL | ids::STONE_HOE
        | ids::STONE_SWORD => 132,
        // 铁
        ids::IRON_PICKAXE | ids::IRON_AXE | ids::IRON_SHOVEL | ids::IRON_HOE | ids::IRON_SWORD => 251,
        // 钻石
        ids::DIAMOND_PICKAXE | ids::DIAMOND_AXE | ids::DIAMOND_SHOVEL | ids::DIAMOND_HOE
        | ids::DIAMOND_SWORD => 1562,
        // 其它
        ids::FLINT_AND_STEEL => 65,
        ids::SHEARS => 239,
        ids::BOW => 385,
        _ => return None,
    };
    Some(d)
}

// 工具类型 用于挖掘效率判定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Sword,
    Shears,
    FlintSteel,
    Bow,
}

pub fn tool_kind(id: i16) -> Option<ToolKind> {
    use ToolKind::*;
    let k = match id {
        ids::WOODEN_PICKAXE | ids::STONE_PICKAXE | ids::IRON_PICKAXE | ids::GOLD_PICKAXE
        | ids::DIAMOND_PICKAXE => Pickaxe,
        ids::WOODEN_AXE | ids::STONE_AXE | ids::IRON_AXE | ids::GOLD_AXE | ids::DIAMOND_AXE => Axe,
        ids::WOODEN_SHOVEL | ids::STONE_SHOVEL | ids::IRON_SHOVEL | ids::GOLD_SHOVEL
        | ids::DIAMOND_SHOVEL => Shovel,
        ids::WOODEN_SWORD | ids::STONE_SWORD | ids::IRON_SWORD | ids::GOLD_SWORD
        | ids::DIAMOND_SWORD => Sword,
        ids::SHEARS => Shears,
        ids::FLINT_AND_STEEL => FlintSteel,
        ids::BOW => Bow,
        _ => return None,
    };
    Some(k)
}

// 工具等级 越高挖得越快 木1 金2 石3 铁4 钻5
pub fn tool_tier(id: i16) -> u8 {
    match id {
        ids::GOLD_PICKAXE | ids::GOLD_AXE | ids::GOLD_SHOVEL | ids::GOLD_HOE | ids::GOLD_SWORD => 2,
        ids::WOODEN_PICKAXE | ids::WOODEN_AXE | ids::WOODEN_SHOVEL | ids::WOODEN_HOE
        | ids::WOODEN_SWORD => 1,
        ids::STONE_PICKAXE | ids::STONE_AXE | ids::STONE_SHOVEL | ids::STONE_HOE
        | ids::STONE_SWORD => 3,
        ids::IRON_PICKAXE | ids::IRON_AXE | ids::IRON_SHOVEL | ids::IRON_HOE | ids::IRON_SWORD => 4,
        ids::DIAMOND_PICKAXE | ids::DIAMOND_AXE | ids::DIAMOND_SHOVEL | ids::DIAMOND_HOE
        | ids::DIAMOND_SWORD => 5,
        _ => 0,
    }
}

// 对物品施加 1 点耐久损耗 返回 true 表示损坏(应从背包移除)
// damage 字段从 0 递增, 达到 max-1 时用尽 (原版 Nukkit: damage >= maxDurability 时 break)
pub fn damage_one(damage: i16, id: i16) -> Option<i16> {
    let max = max_durability(id)?;
    let next = damage + 1;
    if i32::from(next) >= max {
        None // 损坏
    } else {
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durability_matches_original() {
        assert_eq!(max_durability(ids::WOODEN_PICKAXE), Some(60));
        assert_eq!(max_durability(ids::STONE_PICKAXE), Some(132));
        assert_eq!(max_durability(ids::IRON_SWORD), Some(251));
        assert_eq!(max_durability(ids::DIAMOND_PICKAXE), Some(1562));
        assert_eq!(max_durability(ids::GOLD_AXE), Some(33));
        assert_eq!(max_durability(ids::SHEARS), Some(239));
        assert_eq!(max_durability(ids::STONE), None);
    }

    #[test]
    fn tool_tiers_and_kinds() {
        assert_eq!(tool_tier(ids::WOODEN_PICKAXE), 1);
        assert_eq!(tool_tier(ids::DIAMOND_PICKAXE), 5);
        assert_eq!(tool_tier(ids::STONE), 0);
        assert_eq!(tool_kind(ids::IRON_PICKAXE), Some(ToolKind::Pickaxe));
        assert_eq!(tool_kind(ids::SHEARS), Some(ToolKind::Shears));
    }

    #[test]
    fn damage_breaks_at_max() {
        assert_eq!(damage_one(0, ids::WOODEN_PICKAXE), Some(1));
        assert_eq!(damage_one(58, ids::WOODEN_PICKAXE), Some(59));
        assert_eq!(damage_one(59, ids::WOODEN_PICKAXE), None); // 60 耐久 第 60 次损坏
    }
}
