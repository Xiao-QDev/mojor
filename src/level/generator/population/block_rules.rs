//! 对应 `rtg/population/BlockRules.php`：Minecraft 1.12 自然方块规则。
//!
//! 全部按 `state = id << 4 | meta` 判断。装饰物摆放前都靠这几张表决定
//! 「这格能不能长草/仙人掌/睡莲」。注意 `material()` 的分支顺序是有意义的：
//! 106（藤蔓）必须先于 PLANTS 判定，否则藤蔓会被当成普通植物。

use crate::level::generator::population::Pos;
use crate::level::generator::world::GenWorld;

pub const AIR: i32 = 0;
pub const WATER: i32 = 1;
pub const LAVA: i32 = 2;
pub const LEAVES: i32 = 3;
pub const PLANTS: i32 = 4;
pub const ICE: i32 = 5;
pub const SOLID: i32 = 6;
pub const ROCK: i32 = SOLID;
pub const VINE: i32 = 7;
pub const WOOD: i32 = 8;
pub const GRASS: i32 = 9;
pub const GROUND: i32 = 10;
pub const SNOW: i32 = 11;
pub const SAND: i32 = 12;

/// 方块材质分类。
pub fn material(state: i32) -> i32 {
    match state >> 4 {
        0 => AIR,
        8 | 9 => WATER,
        10 | 11 => LAVA,
        18 | 161 => LEAVES,
        79 | 212 => ICE,
        106 => VINE,
        5 | 17 | 162 => WOOD,
        2 => GRASS,
        3 | 60 | 243 => GROUND,
        78 => SNOW,
        12 | 13 => SAND,
        6 | 31 | 32 | 37 | 38 | 39 | 40 | 51 | 59 | 83 | 104 | 105 | 111 | 115 | 127 | 141
        | 142 | 175 | 207 => PLANTS,
        _ => SOLID,
    }
}

pub fn material_is_solid(material: i32) -> bool {
    matches!(
        material,
        SOLID | LEAVES | ICE | WOOD | GRASS | GROUND | SAND
    )
}

pub fn is_solid(state: i32) -> bool {
    material_is_solid(material(state))
}

/// `defaultState`：树叶要带 decay 位（8），否则会自然消失。
pub fn default_state(id: i32) -> i32 {
    (id << 4) | if id == 18 || id == 161 { 8 } else { 0 }
}

/// 木头/树叶：`variant` 0..3 是四种树，4..7 走 161/162（深色橡木、金合欢）。
pub fn wood_state(variant: i32, leaves: bool) -> i32 {
    let id = if leaves {
        if variant < 4 { 18 } else { 161 }
    } else if variant < 4 {
        17
    } else {
        162
    };
    (id << 4) | (variant & 3) | if leaves { 8 } else { 0 }
}

/// RTG 的花枚举：0 = 蒲公英，1..9 = 九种红花变体。
pub fn flower_state(kind: i32) -> i32 {
    if kind == 0 {
        37 << 4
    } else {
        (38 << 4) | (kind - 1)
    }
}

pub fn is_liquid(state: i32) -> bool {
    (8..=11).contains(&(state >> 4))
}

pub fn is_replaceable(world: &GenWorld, pos: Pos) -> bool {
    let state = world.block_state(pos.x, pos.y, pos.z);
    let id = state >> 4;
    if matches!(id, 0 | 8 | 9 | 10 | 11 | 31 | 32 | 51 | 106) {
        return true;
    }
    if id == 78 {
        return state & 7 == 0;
    }
    if id == 175 {
        // 上半格（bit 3）的 variant 存在下半格里
        let variant = if state & 8 != 0 {
            world.block_state(pos.x, pos.y - 1, pos.z) & 7
        } else {
            state & 7
        };
        return variant == 2 || variant == 3;
    }
    false
}

/// 八邻居里有没有水（不含中心）。
pub fn adjacent_water(world: &GenWorld, pos: Pos) -> bool {
    const OFFSETS: [(i32, i32); 8] = [
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
    ];
    OFFSETS.iter().any(|&(dx, dz)| {
        material(world.block_state(pos.x + dx, pos.y, pos.z + dz)) == WATER
    })
}

/// 逐圈扫一个正方形环，要求全是空气。
pub fn area_is_air(world: &GenWorld, pos: Pos, radius: i32) -> bool {
    for i in 1..=radius {
        let mut x = pos.x + i;
        let mut z = pos.z + i;
        while x > pos.x - i {
            x -= 1;
            if world.block_state(x, pos.y, z) >> 4 != 0 {
                return false;
            }
        }
        while z > pos.z - i {
            z -= 1;
            if world.block_state(x, pos.y, z) >> 4 != 0 {
                return false;
            }
        }
        while x < pos.x + i {
            x += 1;
            if world.block_state(x, pos.y, z) >> 4 != 0 {
                return false;
            }
        }
        while z < pos.z + i {
            z += 1;
            if world.block_state(x, pos.y, z) >> 4 != 0 {
                return false;
            }
        }
    }
    true
}

/// 不透明、但又不算「完整方块」的一批（玻璃、冰、台阶等）。
pub fn is_full_block(state: i32) -> bool {
    is_solid(state) && !matches!(state >> 4, 20 | 60 | 79 | 81 | 95 | 102 | 160 | 212)
}

pub fn light_opacity(state: i32) -> i32 {
    match state >> 4 {
        18 | 161 => 1,
        8 | 9 | 79 | 212 => 3,
        _ if is_full_block(state) => 255,
        _ => 0,
    }
}

/// 植物能否长在这一格（`WorldGen` 系列在放置前都会问一遍）。
pub fn can_block_stay(id: i32, world: &GenWorld, pos: Pos) -> bool {
    let below_state = world.block_state(pos.x, pos.y - 1, pos.z);
    let below = below_state >> 4;

    match id {
        // 蘑菇：菌丝或砂土(coarse dirt)，否则要够暗
        39 | 40 => {
            if !(0..256).contains(&pos.y) {
                return false;
            }
            if below == 110 || below == 243 || (below == 3 && below_state & 15 == 2) {
                return true;
            }
            world.light(pos.x, pos.y, pos.z) < 13 && is_full_block(below_state)
        }
        // 甘蔗：旁边要有水，且只能长在沙/土/草上
        83 => {
            if below == 83 {
                return true;
            }
            if below != 2 && below != 3 && below != 12 && below != 243 {
                return false;
            }
            // 注意是「脚下那格」的四邻，不是自身四邻
            let base = pos.down();
            [base.north(), base.south(), base.west(), base.east()]
                .iter()
                .any(|side| {
                    let state = world.block_state(side.x, side.y, side.z);
                    material(state) == WATER || state >> 4 == 212
                })
        }
        // 仙人掌：四邻不能有实心方块或岩浆
        81 => {
            for side in [pos.north(), pos.south(), pos.west(), pos.east()] {
                let state = world.block_state(side.x, side.y, side.z);
                if is_solid(state) || material(state) == LAVA {
                    return false;
                }
            }
            below == 81
                || (below == 12 && !is_liquid(world.block_state(pos.x, pos.y + 1, pos.z)))
        }
        // 枯灌木
        32 => matches!(below, 12 | 172 | 159 | 3 | 243),
        // 花、高草
        31 | 37 | 38 | 175 => below == 2 || below == 3 || below == 60 || below == 243,
        // 睡莲
        111 => {
            if !(0..256).contains(&pos.y) {
                return false;
            }
            let m = material(below_state);
            (m == WATER && below_state & 15 == 0) || m == ICE
        }
        _ => panic!("未实现的植物存活规则: {id}"),
    }
}

pub fn can_place_block_at(id: i32, world: &GenWorld, pos: Pos) -> bool {
    // 西瓜可以替换草/花
    if id == 103 {
        return is_replaceable(world, pos);
    }
    // 南瓜额外要求下面是完整方块
    if id == 86 {
        return is_replaceable(world, pos)
            && is_full_block(world.block_state(pos.x, pos.y - 1, pos.z));
    }
    if id == 83 {
        return can_block_stay(id, world, pos);
    }
    if !world.is_air(pos.x, pos.y, pos.z) {
        return false;
    }
    if id == 111 {
        let state = world.block_state(pos.x, pos.y - 1, pos.z);
        return state >> 4 == 9 || material(state) == ICE;
    }
    if id == 175 && !world.is_air(pos.x, pos.y + 1, pos.z) {
        return false;
    }
    can_block_stay(id, world, pos)
}

/// 对应 `BlockRules::withProperty`：改一个方块状态的属性位。
///
/// 只实现树和装饰用得到的键；`facing` 需要方向枚举，留到 deco 层。
/// 返回 `None` 表示这个 id 没有该属性（PHP 里是抛异常）。
pub fn with_property(state: i32, key: &str, value: i32) -> Option<i32> {
    let id = state >> 4;
    match key {
        "wet" if id == 19 => Some((state & !1) | i32::from(value != 0)),
        "variant" => {
            let value = if id == 161 || id == 162 {
                value - 4
            } else {
                value
            };
            if matches!(id, 17 | 18 | 161 | 162) {
                Some((state & !3) | value)
            } else {
                Some((state & !15) | value)
            }
        }
        // 12 = 树皮全覆盖（Bedrock 的 all-bark 原木）
        "log_axis" if id == 17 || id == 162 => Some((state & !12) | value),
        "check_decay" => Some((state & !8) | if value != 0 { 8 } else { 0 }),
        "decayable" => Some((state & !4) | if value != 0 { 0 } else { 4 }),
        "age" if id == 127 => Some((state & !12) | (value << 2)),
        _ => None,
    }
}

/// 双格植物（175）：`2800|variant` 是下半格，上半格要带同一个 variant
/// （`2808|variant`），否则所有双格植物都会渲染成向日葵。
pub fn place_at(id: i32, world: &mut GenWorld, pos: Pos, variant: i32) {
    assert_eq!(id, 175, "只实现了双格植物");
    world.set_block_state(pos.x, pos.y, pos.z, 2800 | (variant & 7));
    world.set_block_state(pos.x, pos.y + 1, pos.z, 2808 | (variant & 7));
}

/// 藤蔓（106）指定朝向的 state。MC 1.8 vine meta 是朝向位掩码：
/// south=1, west=2, north=4, east=8。
pub fn vine_with_face(face: &str) -> i32 {
    let bit = match face {
        "south" => 1,
        "west" => 2,
        "north" => 4,
        "east" => 8,
        _ => 0,
    };
    (106 << 4) | bit
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::level::format::chunk::Chunk;

    fn world_with(blocks: &[(Pos, i32)]) -> GenWorld<'static> {
        let chunks: &'static mut HashMap<(i32, i32), Chunk> =
            Box::leak(Box::new(HashMap::new()));
        chunks.insert((0, 0), Chunk::empty(0, 0));
        for &(pos, state) in blocks {
            let chunk = chunks.get_mut(&(0, 0)).unwrap();
            chunk.set_block(pos.x as usize, pos.y as usize, pos.z as usize, (state >> 4) as u8);
            chunk.set_meta(pos.x as usize, pos.y as usize, pos.z as usize, (state & 15) as u8);
        }
        GenWorld::new(chunks)
    }

    #[test]
    fn material_families() {
        assert_eq!(material(0), AIR);
        assert_eq!(material(9 << 4), WATER);
        assert_eq!(material(11 << 4), LAVA);
        assert_eq!(material(18 << 4), LEAVES);
        assert_eq!(material(106 << 4), VINE);
        assert_eq!(material(17 << 4), WOOD);
        assert_eq!(material(2 << 4), GRASS);
        assert_eq!(material(3 << 4), GROUND);
        assert_eq!(material(243 << 4), GROUND);
        assert_eq!(material(78 << 4), SNOW);
        assert_eq!(material(12 << 4), SAND);
        assert_eq!(material(37 << 4), PLANTS);
        assert_eq!(material(1 << 4), SOLID);
        assert!(is_solid(1 << 4));
        assert!(!is_solid(0));
    }

    #[test]
    fn full_block_excludes_glass_and_slabs() {
        assert!(is_full_block(1 << 4));
        assert!(!is_full_block(20 << 4));
        assert!(!is_full_block(79 << 4));
        assert_eq!(light_opacity(18 << 4), 1);
        assert_eq!(light_opacity(9 << 4), 3);
        assert_eq!(light_opacity(1 << 4), 255);
        assert_eq!(light_opacity(0), 0);
    }

    #[test]
    fn flower_and_wood_states() {
        assert_eq!(flower_state(0), 37 << 4);
        assert_eq!(flower_state(3), (38 << 4) | 2);
        assert_eq!(wood_state(1, false), (17 << 4) | 1);
        assert_eq!(wood_state(5, false), (162 << 4) | 1);
        assert_eq!(wood_state(2, true), (18 << 4) | 2 | 8);
        assert_eq!(wood_state(6, true), (161 << 4) | 2 | 8);
    }

    #[test]
    fn cactus_needs_sand_and_clear_sides() {
        // 沙上、四周空
        let world = world_with(&[(Pos::new(0, 63, 0), 12 << 4)]);
        assert!(can_block_stay(81, &world, Pos::new(0, 64, 0)));
        // 旁边有实心方块就不行
        let world = world_with(&[
            (Pos::new(0, 63, 0), 12 << 4),
            (Pos::new(1, 64, 0), 1 << 4),
        ]);
        assert!(!can_block_stay(81, &world, Pos::new(0, 64, 0)));
    }

    #[test]
    fn sugar_cane_needs_water_neighbour() {
        let dry = world_with(&[(Pos::new(0, 63, 0), 12 << 4)]);
        assert!(!can_block_stay(83, &dry, Pos::new(0, 64, 0)));
        let wet = world_with(&[
            (Pos::new(0, 63, 0), 12 << 4),
            (Pos::new(1, 63, 0), 9 << 4),
        ]);
        assert!(can_block_stay(83, &wet, Pos::new(0, 64, 0)));
    }

    #[test]
    fn lily_pad_needs_still_water() {
        let still = world_with(&[(Pos::new(0, 62, 0), 9 << 4)]);
        assert!(can_block_stay(111, &still, Pos::new(0, 63, 0)));
        let ice = world_with(&[(Pos::new(0, 62, 0), 79 << 4)]);
        assert!(can_block_stay(111, &ice, Pos::new(0, 63, 0)));
        let air = world_with(&[]);
        assert!(!can_block_stay(111, &air, Pos::new(0, 63, 0)));
    }

    #[test]
    fn area_is_air_scans_ring() {
        let empty = world_with(&[]);
        assert!(area_is_air(&empty, Pos::new(8, 64, 8), 2));
        let blocked = world_with(&[(Pos::new(10, 64, 8), 1 << 4)]);
        assert!(!area_is_air(&blocked, Pos::new(8, 64, 8), 2));
    }

    // 上半格必须带同一个 variant 否则所有双格植物都会渲染成向日葵。
    #[test]
    fn double_plant_upper_half_keeps_variant() {
        let mut world = world_with(&[(Pos::new(0, 63, 0), 2 << 4)]);
        place_at(175, &mut world, Pos::new(0, 64, 0), 3);
        assert_eq!(world.block_state(0, 64, 0), 2803, "大型蕨下半格应为 175:3");
        assert_eq!(
            world.block_state(0, 65, 0),
            2811,
            "大型蕨上半格应为 175:11(8|3)，不能是固定的向日葵 175:8"
        );
    }
}
