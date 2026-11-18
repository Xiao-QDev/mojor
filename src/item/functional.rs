// 功能物品 照搬 MCPE 0.14.1 / ITX 各 *Item::onActivate
// 纯决策逻辑: 给定手持物品 + 被点方块(target, onActivate 的对象) + 放置格(place, 面相邻格)
// 产出方块改动与物品替换. region 线程调用后写进 ChunkManager 并同步背包.
//
// 支持: 空桶装水/岩浆, 水桶/岩浆桶放置, 打火石点火, 锄头耕地, 骨粉催熟
use super::ids;
use crate::level::generator::blocks as b;
use crate::network::protocol::item::ItemStack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSet {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub id: u8,
    pub meta: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseResult {
    pub sets: Vec<BlockSet>,
    // 手持物品替换: None=不变, Some(None)=消耗掉, Some(Some(stack))=变成别的
    pub replace: Option<Option<ItemStack>>,
    // 是否消耗 1 点工具耐久
    pub damage_tool: bool,
}

impl UseResult {
    fn none() -> Self {
        Self { sets: Vec::new(), replace: None, damage_tool: false }
    }
}

// 桶 damage 值 与 Nukkit ItemBucket 一致
const BUCKET_EMPTY: i16 = 0;
const BUCKET_WATER: i16 = 8;
const BUCKET_LAVA: i16 = 10;

fn is_hoe(id: i16) -> bool {
    matches!(id, ids::WOODEN_HOE | ids::STONE_HOE | ids::IRON_HOE | ids::GOLD_HOE | ids::DIAMOND_HOE)
}

// 骨粉 = 染料 damage 15 (WHITE) 与 ITX Dye::WHITE=15 一致
fn is_bone_meal(item: ItemStack) -> bool {
    item.id == ids::DYE && item.damage == 15
}

// 该物品是否走功能物品分支(而非普通放置) 用于在 on_use_item 入口分流
pub fn is_functional(id: i16, damage: i16) -> bool {
    if id == ids::BUCKET {
        // 空桶/水桶/岩浆桶都走功能分支; 奶桶(1)暂按普通放置
        return matches!(damage, BUCKET_EMPTY | BUCKET_WATER | BUCKET_LAVA);
    }
    id == ids::FLINT_AND_STEEL || is_hoe(id) || (id == ids::DYE && damage == 15)
}

fn mature_meta(id: u8) -> Option<u8> {
    match id {
        b::WHEAT_BLOCK => Some(7),
        b::CARROT_BLOCK => Some(7),
        b::POTATO_BLOCK => Some(7),
        b::BEETROOT_BLOCK => Some(3),
        b::PUMPKIN_STEM => Some(7),
        b::MELON_STEM => Some(7),
        _ => None,
    }
}

// target: 被点的格子 (onActivate 目标) 坐标 + id/meta
// place: 点击面相邻格 (放置位置) 坐标 + 该格当前 id
// place_below_solid: 放置格下方是否实心 (打火石要)
pub fn use_item_on(
    item: ItemStack,
    target: (i32, i32, i32),
    target_id: u8,
    target_meta: u8,
    place: (i32, i32, i32),
    place_block: u8,
    place_below_solid: bool,
) -> Option<UseResult> {
    // 空桶装液体: 被点是液面(源 meta=0) -> 原格变空气, 桶变成对应满桶
    if item.id == ids::BUCKET && item.damage == BUCKET_EMPTY && target_meta == 0 {
        let filled = if target_id == b::STILL_WATER || target_id == b::WATER {
            Some(BUCKET_WATER)
        } else if target_id == b::STILL_LAVA || target_id == b::LAVA {
            Some(BUCKET_LAVA)
        } else {
            None
        };
        if let Some(filled) = filled {
            let mut r = UseResult::none();
            r.sets.push(BlockSet { x: target.0, y: target.1, z: target.2, id: b::AIR, meta: 0 });
            r.replace = Some(Some(ItemStack { id: ids::BUCKET, count: 1, damage: filled }));
            return Some(r);
        }
    }
    // 满桶放置液体源: 放置格可替换
    if item.id == ids::BUCKET && place_block == b::AIR {
        let liquid = if item.damage == BUCKET_WATER {
            Some(b::STILL_WATER)
        } else if item.damage == BUCKET_LAVA {
            Some(b::STILL_LAVA)
        } else {
            None
        };
        if let Some(liquid) = liquid {
            let mut r = UseResult::none();
            r.sets.push(BlockSet { x: place.0, y: place.1, z: place.2, id: liquid, meta: 0 });
            r.replace = Some(Some(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_EMPTY }));
            return Some(r);
        }
    }
    // 打火石: 放置格空气 + 下方实心 -> 点火, 消耗 1 耐久
    if item.id == ids::FLINT_AND_STEEL && place_block == b::AIR && place_below_solid {
        let mut r = UseResult::none();
        r.sets.push(BlockSet { x: place.0, y: place.1, z: place.2, id: b::FIRE, meta: 0 });
        r.damage_tool = true;
        return Some(r);
    }
    // 锄头: 被点草/泥土 -> 耕地, 消耗 1 耐久
    if is_hoe(item.id) && matches!(target_id, b::GRASS | b::DIRT) {
        let mut r = UseResult::none();
        r.sets.push(BlockSet { x: target.0, y: target.1, z: target.2, id: b::FARMLAND, meta: 0 });
        r.damage_tool = true;
        return Some(r);
    }
    // 骨粉催熟: 被点是未成熟作物
    if is_bone_meal(item) {
        if let Some(mature) = mature_meta(target_id) {
            if target_meta < mature {
                let mut r = UseResult::none();
                r.sets.push(BlockSet { x: target.0, y: target.1, z: target.2, id: target_id, meta: mature });
                return Some(r);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_bucket_picks_up_water() {
        let r = use_item_on(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_EMPTY }, (0, 4, 0), b::STILL_WATER, 0, (0, 5, 0), b::AIR, true).unwrap();
        assert_eq!(r.sets[0], BlockSet { x: 0, y: 4, z: 0, id: b::AIR, meta: 0 });
        assert_eq!(r.replace, Some(Some(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_WATER })));
    }

    #[test]
    fn empty_bucket_picks_up_lava() {
        let r = use_item_on(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_EMPTY }, (2, 4, 3), b::STILL_LAVA, 0, (2, 5, 3), b::AIR, true).unwrap();
        assert_eq!(r.replace, Some(Some(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_LAVA })));
    }

    #[test]
    fn water_bucket_places_source() {
        let r = use_item_on(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_WATER }, (0, 4, 0), b::STONE, 0, (1, 5, 1), b::AIR, true).unwrap();
        assert_eq!(r.sets[0], BlockSet { x: 1, y: 5, z: 1, id: b::STILL_WATER, meta: 0 });
        assert_eq!(r.replace, Some(Some(ItemStack { id: ids::BUCKET, count: 1, damage: BUCKET_EMPTY })));
    }

    #[test]
    fn flint_and_steel_ignites_on_solid() {
        let r = use_item_on(ItemStack { id: ids::FLINT_AND_STEEL, count: 1, damage: 0 }, (0, 4, 0), b::STONE, 0, (0, 5, 0), b::AIR, true).unwrap();
        assert_eq!(r.sets[0].id, b::FIRE);
        assert!(r.damage_tool);
        assert!(use_item_on(ItemStack { id: ids::FLINT_AND_STEEL, count: 1, damage: 0 }, (0, 4, 0), b::STONE, 0, (0, 5, 0), b::AIR, false).is_none());
    }

    #[test]
    fn hoe_tills_grass() {
        let r = use_item_on(ItemStack { id: ids::WOODEN_HOE, count: 1, damage: 0 }, (0, 4, 0), b::GRASS, 0, (0, 5, 0), b::AIR, true).unwrap();
        assert_eq!(r.sets[0], BlockSet { x: 0, y: 4, z: 0, id: b::FARMLAND, meta: 0 });
        assert!(r.damage_tool);
    }

    #[test]
    fn bone_meal_grows_wheat() {
        let bone = ItemStack { id: ids::DYE, count: 1, damage: 15 };
        let r = use_item_on(bone, (0, 5, 0), b::WHEAT_BLOCK, 2, (0, 6, 0), b::AIR, true).unwrap();
        assert_eq!(r.sets[0].meta, 7);
        assert!(use_item_on(bone, (0, 5, 0), b::WHEAT_BLOCK, 7, (0, 6, 0), b::AIR, true).is_none());
    }
}
