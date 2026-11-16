// 挖方块的掉落物 照搬 Nukkit 各 Block::getDrops 的覆盖规则
// 默认 Block::getDrops 是 {方块 id, 整个 meta, 1}(羊毛/石头变种靠它)
// 但很多方块的 meta 里有状态位(原木轴向 台阶上下半 门上下半) 直接带进物品 damage 会串种类
use crate::item::ids;
use crate::network::protocol::item::ItemStack;

fn stack(id: i16, damage: i16) -> ItemStack {
    ItemStack { id, count: 1, damage }
}

// 楼梯 id 表 与 placement.rs 的 STAIRS 一致
const STAIRS: &[u8] = &[
    53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180, 203,
];

// 木门方块 -> 门物品 铁门用镐才掉 这里一律掉
const WOOD_DOORS: &[(u8, i16)] = &[
    (64, ids::WOODEN_DOOR),
    (193, 427),
    (194, 428),
    (195, 429),
    (196, 430),
    (197, 431),
];

// 用坐标做确定性伪随机 替代 Nukkit 的 Math.random 概率掉落
fn chance(x: i32, y: i32, z: i32, salt: u32, out_of: u32) -> bool {
    let mut h = (x as u32)
        .wrapping_mul(0x9E3779B1)
        ^ (y as u32).wrapping_mul(0x85EBCA6B)
        ^ (z as u32).wrapping_mul(0xC2B2AE35)
        ^ salt;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB352D);
    h ^= h >> 15;
    h % out_of == 0
}

// 返回要掉的物品 可能一个都没有(树叶空手 或方块本身不掉)
pub fn drops_for(id: u8, meta: u8, x: i32, y: i32, z: i32) -> Vec<ItemStack> {
    // 原木 低两位是树种 高两位是轴向 挖下来只留树种
    if id == 17 || id == 162 {
        return vec![stack(id as i16, (meta & 0x03) as i16)];
    }
    // 树叶 空手概率掉苹果(仅橡树)或树苗 剪刀情况由手持物决定 这里按空手处理
    if id == 18 || id == 161 {
        let species = (meta & 0x03) as i16;
        if id == 18 && species == 0 && chance(x, y, z, 1, 200) {
            return vec![stack(ids::APPLE, 0)];
        }
        if chance(x, y, z, 2, 20) {
            return vec![stack(ids::SAPLING, species)];
        }
        return Vec::new();
    }
    // 台阶 低三位是材质 0x08 是上半格位
    if id == 44 || id == 158 || id == 182 {
        return vec![stack(id as i16, (meta & 0x07) as i16)];
    }
    // 楼梯 朝向位不能进物品 damage
    if STAIRS.contains(&id) {
        return vec![stack(id as i16, 0)];
    }
    // 砂岩 低两位是种类
    if id == 24 {
        return vec![stack(ids::SANDSTONE, (meta & 0x03) as i16)];
    }
    // 门 方块 meta 是上下半/朝向 掉落的是门物品
    if let Some((_, item)) = WOOD_DOORS.iter().find(|(block, _)| *block == id) {
        return vec![stack(*item, 0)];
    }
    if id == 71 {
        return vec![stack(330, 0)]; // 铁门物品
    }
    // 耕地 / 泥土 一律掉普通泥土
    if id == 60 || id == 3 {
        return vec![stack(ids::DIRT, 0)];
    }
    // 仙人掌 / 切石机 状态位清掉
    if id == 81 || id == 245 {
        return vec![stack(id as i16, 0)];
    }
    // 雪层 掉雪球而不是雪层方块
    if id == 78 {
        return vec![stack(ids::SNOWBALL, 0)];
    }
    // 默认 方块 id + 完整 meta(羊毛色 石头变种等)
    vec![stack(id as i16, meta as i16)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oak_log_drops_only_species_bits() {
        // 橡木 + 横放轴向位 0x08 挖下来仍是橡木 damage 0
        assert_eq!(drops_for(17, 0x08, 0, 70, 0)[0].damage, 0);
        // 白桦 + 轴向位
        assert_eq!(drops_for(17, 0x0A, 0, 70, 0)[0].damage, 2);
        assert_eq!(drops_for(162, 0x04, 0, 70, 0)[0].damage, 0);
    }

    #[test]
    fn wool_keeps_full_meta_but_slab_masks_state() {
        assert_eq!(drops_for(35, 14, 0, 70, 0)[0].damage, 14);
        assert_eq!(drops_for(44, 0x0B, 0, 70, 0)[0].damage, 3);
        assert_eq!(drops_for(53, 0x05, 0, 70, 0)[0].damage, 0);
    }

    #[test]
    fn doors_and_farmland_drop_items() {
        assert_eq!(drops_for(64, 0x0C, 0, 70, 0)[0].id, ids::WOODEN_DOOR);
        assert_eq!(drops_for(60, 7, 0, 70, 0)[0].id, ids::DIRT);
        assert_eq!(drops_for(78, 3, 0, 70, 0)[0].id, ids::SNOWBALL);
    }
}
