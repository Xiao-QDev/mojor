//! 结构 Feature（MCPE 0.14.1 `levelgen/feature/MonsterRoomFeature.cpp` 与
//! `levelgen/structure/WeighedTreasureItem.cpp`）。
//!
//! 目前只有地牢。村庄、废弃矿井、要塞、神殿还没有移植，它们会在地牢之前消耗同一个随机数，
//! 所以装饰阶段的地牢位置在含这些结构的区块里可能与原版不同。

use super::blockprops as bp;
use super::features::{Src, AIR, COBBLE, MOSSY_COBBLE};
use crate::level::generator::mcpe_noise::McpeRandom;

const CHEST: u8 = 54;
const MOB_SPAWNER: u8 = 52;

/// `WeighedTreasureItem::addChestItems` 只看随机数序列：每个条目 (权重, min, max)。
/// 物品内容不影响地形，这里只保留随机数消耗。
const TREASURE: [(i32, i32, i32); 6] = [(10, 1, 4), (10, 1, 1), (10, 1, 4), (10, 1, 4), (10, 1, 4), (5, 1, 1)];

/// `Math::nextInt(random, lower, upper)`：lower >= upper 时不消耗随机数
fn math_next_int(r: &mut McpeRandom, lower: i32, upper: i32) -> i32 {
    if lower >= upper {
        return lower;
    }
    lower + r.next_int((upper - lower + 1) as u32) as i32
}

/// `WeighedTreasureItem::addChestItems(random, items, container, count)`
/// 每轮：加权抽取 → 数量（可能消耗）→ `random.nextInt(容量)` 选槽位。
/// 数量最多 4，不会超过堆叠上限，所以没有多堆分摊分支。
pub fn add_chest_items(r: &mut McpeRandom, container_size: u32, count: i32) {
    let total: i32 = TREASURE.iter().map(|t| t.0).sum();
    for _ in 0..count {
        let mut remainder = r.next_int(total as u32) as i32;
        let mut picked = None;
        for t in TREASURE.iter() {
            remainder -= t.0;
            if remainder < 0 {
                picked = Some(*t);
                break;
            }
        }
        let Some((_, lo, hi)) = picked else { continue };
        let _wanted = math_next_int(r, lo, hi);
        let _slot = r.next_int(container_size);
    }
}

/// 同 `Src::set` 的 `FullBlock` 写入；`removeBlock` 等价于写空气
fn chest_is_large(_s: &Src, _x: i32, _y: i32, _z: i32) -> bool {
    // 地牢里两个宝箱只用 flag 2 写入、不触发邻居更新，不会配对（见 ChestBlockEntity::onNeighborChanged）。
    // 待与原版对拍确认。
    false
}

/// `MonsterRoomFeature::place`。返回是否放置成功。
pub fn monster_room(s: &mut Src, pos: (i32, i32, i32), r: &mut McpeRandom) -> bool {
    let (px, py, pz) = pos;
    let side_x = r.next_int(2) as i32 + 2;
    let side_z = r.next_int(2) as i32 + 2;
    let mut openings = 0;
    for x in px - side_x - 1..=px + side_x + 1 {
        for y in py - 1..=py + 4 {
            for z in pz - side_z - 1..=pz + side_z + 1 {
                let solid = bp::mat_solid(s.get(x, y, z));
                if y == py - 1 && !solid {
                    return false;
                }
                if y == py + 4 && !solid {
                    return false;
                }
                if (x == px - side_x - 1 || x == px + side_x + 1 || z == pz - side_z - 1 || z == pz + side_z + 1)
                    && y == py
                    && s.is_empty(x, y, z)
                    && s.is_empty(x, y + 1, z)
                {
                    openings += 1;
                }
            }
        }
    }
    if !(1..=5).contains(&openings) {
        return false;
    }

    for x in px - side_x - 1..=px + side_x + 1 {
        for y in (py - 1..=py + 3).rev() {
            for z in pz - side_z - 1..=pz + side_z + 1 {
                let boundary = x == px - side_x - 1
                    || y == py - 1
                    || z == pz - side_z - 1
                    || x == px + side_x + 1
                    || y == py + 4
                    || z == pz + side_z + 1;
                if boundary {
                    // y < 0 时原版短路为真；这里 y >= 0 恒成立（py-1 >= 0 由调用方保证）
                    if y < 0 || bp::mat_solid(s.get(x, y - 1, z)) {
                        if bp::mat_solid(s.get(x, y, z)) {
                            if y == py - 1 && r.next_int(4) != 0 {
                                s.set(x, y, z, MOSSY_COBBLE, 0);
                            } else {
                                s.set(x, y, z, COBBLE, 0);
                            }
                        }
                    } else {
                        s.set(x, y, z, AIR, 0);
                    }
                } else {
                    s.set(x, y, z, AIR, 0);
                }
            }
        }
    }

    for _chest in 0..2 {
        for _attempt in 0..3 {
            let off_x = r.next_int((2 * side_x + 1) as u32) as i32 - side_x;
            let off_z = r.next_int((2 * side_z + 1) as u32) as i32 - side_z;
            let (cx, cz) = (px + off_x, pz + off_z);
            if !s.is_empty(cx, py, cz) {
                continue;
            }
            let mut solid = 0;
            for (ax, az) in [(cx - 1, cz), (cx + 1, cz), (cx, cz - 1), (cx, cz + 1)] {
                if bp::mat_solid(s.get(ax, py, az)) {
                    solid += 1;
                }
            }
            if solid != 1 {
                continue;
            }
            s.set(cx, py, cz, CHEST, 0);
            let size = if chest_is_large(s, cx, py, cz) { 54 } else { 27 };
            add_chest_items(r, size, 8);
            break;
        }
    }

    s.set(px, py, pz, MOB_SPAWNER, 0);
    // _getRandomEntityId：四个实体之一
    let _entity = r.next_int(4);
    true
}

/// `RandomLevelSource::postProcess` 里的地牢段（8 次尝试）
pub fn dungeons(s: &mut Src, rng: &mut McpeRandom, base_x: i32, base_z: i32) {
    for _ in 0..8 {
        let x = base_x + rng.next_int(16) as i32 + 8;
        let y = rng.next_int(128) as i32;
        let z = base_z + rng.next_int(16) as i32 + 8;
        monster_room(s, (x, y, z), rng);
    }
}
