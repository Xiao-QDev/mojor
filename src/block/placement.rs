// 手持物品 -> 实际放下的方块 对照 Nukkit 0.14 的 Item 子类 this.block 与各方块 place()
//
// 客户端 UseItem 包里带的是物品 id 物品表里的方块类物品 id 与方块 id 相同(都 < 256)
// 但门/床/告示牌/蛋糕/甘蔗/作物/花盆 这些在物品表里是独立 id(>= 256)
// 直接拿 item_id 截断成 u8 会放错方块 床 355 -> 99 巨大蘑菇块 橡木门 324 -> 68 墙上告示牌
//
// 一次的放置结果最多两格(门/床 是上下/前后两格)
// 这里只算 id 与 meta 目标格能不能放(实心支撑 可替换)由世界层判定

use crate::level::generator::blocks as b;

// 一次放置里的一格 坐标是相对目标格的偏移
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
    pub id: u8,
    pub meta: u8,
}

// 放置结果 first 一定要写 second 是门/床的第二格
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub first: Cell,
    pub second: Option<Cell>,
}

// 放置上下文 全部来自 UseItem 包与玩家状态
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub item_id: i16,
    // 物品的 damage 铁轨 羊毛色 木头种类 台阶材质 都在这
    pub item_meta: i16,
    pub face: u8,
    pub yaw: f32,
    // 命中点在目标方块内的 y 0..1 台阶/楼梯 靠它判上下半格
    pub fy: f32,
}

impl Placement {
    // 按绝对坐标遍历 第一格就是 target 本身 其余格按偏移推
    pub fn iter_abs(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> impl Iterator<Item = (i32, i32, i32, Cell)> {
        let second = self
            .second
            .map(|c| (x + c.dx, y + c.dy, z + c.dz, c));
        std::iter::once((x, y, z, self.first)).chain(second)
    }

    // 单格放置 测试与普通方块走这条
    pub fn single(id: u8, meta: u8) -> Self {
        Self {
            first: Cell {
                dx: 0,
                dy: 0,
                dz: 0,
                id,
                meta,
            },
            second: None,
        }
    }
}

// 方块面的偏移 0 下 1 上 2 北 3 南 4 西 5 东 与 UseItemPacket::target_block 一致
const DELTA: [(i32, i32); 6] = [
    (0, -1), // 0 下
    (0, 1),  // 1 上
    (0, -1), // 2 北 z-1
    (0, 1),  // 3 南 z+1
    (-1, 0), // 4 西 x-1
    (1, 0),  // 5 东 x+1
];

// Nukkit 里 direction -> 面的映射 BlockDoor/BlockBed/BlockStairs 共用
// 0 南 1 西 2 北 3 东
const DIR_FACE: [u8; 4] = [3, 4, 2, 5];

// 门邻居的 face Nukkit BlockDoor::place faces[] = {3, 4, 2, 5}
// 第一个是铰链侧 第二个是门正面 都是相对目标格的水平偏移
pub fn door_faces(direction: u8) -> (u8, u8) {
    let dir = (direction & 0x03) as usize;
    (DIR_FACE[(dir + 2) % 4], DIR_FACE[dir])
}

// face 对应的水平偏移 dx dz
pub fn face_offset(face: u8) -> (i32, i32) {
    DELTA[face as usize]
}

// 楼梯方向的查表 BlockStairs::place
const STAIR_FACE: [u8; 4] = [0, 2, 1, 3];

// 原木的轴向位 BlockWood::place faces[] = {0, 0, 0b1000, 0b1000, 0b0100, 0b0100}
const LOG_AXIS: [u8; 6] = [0, 0, 0b1000, 0b1000, 0b0100, 0b0100];

// 熔炉正面朝向 BlockFurnaceBurning::place faces[] = {4, 2, 5, 3}
// 索引是玩家方向 direction(): 0=南 1=西 2=北 3=东
const FURNACE_FACE: [u8; 4] = [4, 2, 5, 3];

// 火把贴在侧面时的 meta BlockTorch::place faces[] = {0, 5, 4, 3, 2, 1}
// 点顶面/底面给站立火把(0) 免得墙上凭空立一根
const TORCH_META: [u8; 6] = [0, 0, 4, 3, 2, 1];

// 楼梯 木/石/砖/石砖/地狱砖/砂岩/云杉/白桦/丛林/石英/金合欢/深色橡木/红砂岩/紫珀
const STAIRS: [u8; 14] = [53, 67, 108, 109, 114, 128, 134, 135, 136, 156, 163, 164, 180, 203];

// 台阶 石台阶 木台阶 红砂岩台阶
const SLABS: [u8; 3] = [b::SLAB, b::WOOD_SLAB, b::RED_SANDSTONE_SLAB];

// Nukkit Entity::getDirection() 0 南 1 西 2 北 3 东
pub fn direction(yaw: f32) -> u8 {
    let mut rotation = (yaw - 90.0) % 360.0;
    if rotation < 0.0 {
        rotation += 360.0;
    }
    if rotation < 45.0 || rotation >= 315.0 {
        2 // 北
    } else if rotation < 135.0 {
        3 // 东
    } else if rotation < 225.0 {
        0 // 南
    } else {
        1 // 西
    }
}

// 手持物品能放成什么 放不了返回 None
pub fn resolve(c: &Context) -> Option<Placement> {
    if c.item_id <= 0 || c.face > 5 {
        return None;
    }
    // 物品表里另有 id 的那些方块 对照 Nukkit 的 Item 子类
    let id = match c.item_id {
        295 => b::WHEAT_BLOCK,        // ItemSeedsWheat -> BlockWheat
        323 => return sign(c),        // ItemSign -> BlockSignPost / BlockWallSign
        324 => return door(c, b::WOOD_DOOR_BLOCK), // ItemDoorWood
        330 => return door(c, b::IRON_DOOR_BLOCK), // ItemDoorIron
        331 => b::REDSTONE_WIRE,      // ItemRedstone -> BlockRedstoneWire
        338 => b::SUGARCANE,          // ItemSugarcane
        354 => b::CAKE_BLOCK,         // ItemCake
        355 => return bed(c),         // ItemBed
        361 => b::PUMPKIN_STEM,       // ItemSeedsPumpkin
        362 => b::MELON_STEM,         // ItemSeedsMelon
        379 => b::BREWING_STAND_BLOCK,// ItemBrewingStand
        380 => b::CAULDRON_BLOCK,     // ItemCauldron
        390 => b::FLOWER_POT_BLOCK,   // ItemFlowerPot
        391 => b::CARROT_BLOCK,       // ItemCarrot
        392 => b::POTATO_BLOCK,       // ItemPotato
        397 => b::SKULL_BLOCK,        // ItemSkull
        427 => return door(c, b::SPRUCE_DOOR_BLOCK),    // ItemDoorSpruce
        428 => return door(c, b::BIRCH_DOOR_BLOCK),     // ItemDoorBirch
        429 => return door(c, b::JUNGLE_DOOR_BLOCK),    // ItemDoorJungle
        430 => return door(c, b::ACACIA_DOOR_BLOCK),    // ItemDoorAcacia
        431 => return door(c, b::DARK_OAK_DOOR_BLOCK),  // ItemDoorDarkOak
        458 => b::BEETROOT_BLOCK,     // ItemSeedsBeetroot -> BlockBeetroot
        n if (1..256).contains(&n) => n as u8,
        _ => return None,
    };
    Some(Placement::single(id, meta_for(id, c)))
}

// 方块目标的 meta 规则 照各方块的 place()
fn meta_for(id: u8, c: &Context) -> u8 {
    if STAIRS.contains(&id) {
        // BlockStairs 朝向 + 是否倒置
        let mut meta = STAIR_FACE[direction(c.yaw) as usize] & 0x03;
        if (c.fy > 0.5 && c.face != 1) || c.face == 0 {
            meta |= 0x04;
        }
        return meta;
    }
    match id {
        // BlockWood 轴向
        b::LOG | b::LOG2 => (c.item_meta as u8 & 0x03) | LOG_AXIS[c.face as usize],
        // BlockSlab 上半格位
        _ if SLABS.contains(&id) => {
            let mut meta = c.item_meta as u8 & 0x07;
            if c.face == 0 || (c.face != 1 && c.fy > 0.5) {
                meta |= 0x08;
            }
            meta
        }
        // BlockTorch 贴面方向
        b::TORCH => TORCH_META[c.face as usize],
        // 熔炉/燃烧中的熔炉 正面朝向玩家 与点击面和物品 damage 无关
        b::FURNACE | b::BURNING_FURNACE => FURNACE_FACE[(direction(c.yaw) & 0x03) as usize],
        // 其余方块 物品 damage 就是方块 meta 羊毛色 石头种类 木头种类 都走这条
        _ => c.item_meta as u8 & 0x0f,
    }
}

// 告示牌 BlockSignPost::place 顶面立牌(63) 侧面挂牌(68)
fn sign(c: &Context) -> Option<Placement> {
    if c.face == 0 {
        return None;
    }
    if c.face == 1 {
        let meta = (((c.yaw + 180.0) * 16.0 / 360.0) + 0.5).floor() as i32 & 0x0f;
        Some(Placement::single(b::SIGN_POST, meta as u8))
    } else {
        Some(Placement::single(b::WALL_SIGN, c.face))
    }
}

// 门 BlockDoor::place 只能点顶面 下半 meta 是朝向 上半只放 0x08
// 铰链位(0x01)要看两侧是不是已经有一扇门 这里先不判
fn door(c: &Context, id: u8) -> Option<Placement> {
    if c.face != 1 {
        return None;
    }
    let dir = direction(c.yaw) & 0x03;
    Some(Placement {
        first: Cell {
            dx: 0,
            dy: 0,
            dz: 0,
            id,
            meta: dir,
        },
        second: Some(Cell {
            dx: 0,
            dy: 1,
            dz: 0,
            id,
            meta: 0x08,
        }),
    })
}

// 床 BlockBed::place 朝向 + 3 的那一侧放床头(meta|0x08)
// meta 存的是方向下标 0..3 不是方块面
fn bed(c: &Context) -> Option<Placement> {
    let index = ((direction(c.yaw) as usize) + 3) % 4;
    let (dx, dz) = DELTA[DIR_FACE[index] as usize];
    let meta = index as u8 & 0x03;
    Some(Placement {
        first: Cell {
            dx: 0,
            dy: 0,
            dz: 0,
            id: b::BED_BLOCK,
            meta,
        },
        second: Some(Cell {
            dx,
            dy: 0,
            dz,
            id: b::BED_BLOCK,
            meta: meta | 0x08,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(item_id: i16, item_meta: i16, face: u8, yaw: f32, fy: f32) -> Context {
        Context {
            item_id,
            item_meta,
            face,
            yaw,
            fy,
        }
    }

    #[test]
    fn plain_block_keeps_item_id_and_damage() {
        // 羊毛 35 damage 14 红色
        let p = resolve(&ctx(35, 14, 1, 0.0, 0.5)).unwrap();
        assert_eq!((p.first.id, p.first.meta), (35, 14));
        // 石头 1
        let p = resolve(&ctx(1, 0, 1, 0.0, 0.5)).unwrap();
        assert_eq!((p.first.id, p.first.meta), (1, 0));
        assert!(p.second.is_none());
    }

    #[test]
    fn item_only_blocks_map_to_real_block() {
        // 床 355 -> 26 双格 yaw 0 -> direction 1 西 (1+3)%4 = 0 床头在南侧 z+1
        let p = resolve(&ctx(355, 0, 1, 0.0, 0.5)).unwrap();
        assert_eq!(p.first.id, b::BED_BLOCK);
        assert_eq!(p.first.meta, 0);
        assert_eq!(p.second.unwrap().id, b::BED_BLOCK);
        assert_eq!((p.second.unwrap().dz, p.second.unwrap().meta), (1, 0x08));
        // 橡木门 324 -> 64 上下两格
        let p = resolve(&ctx(324, 0, 1, 0.0, 0.5)).unwrap();
        assert_eq!(p.first.id, b::WOOD_DOOR_BLOCK);
        assert_eq!(p.second.unwrap().dy, 1);
        assert_eq!(p.second.unwrap().meta, 0x08);
        // 蛋糕 354 -> 92
        assert_eq!(resolve(&ctx(354, 0, 1, 0.0, 0.5)).unwrap().first.id, 92);
        // 花盆 390 -> 140
        assert_eq!(resolve(&ctx(390, 0, 1, 0.0, 0.5)).unwrap().first.id, 140);
        // 甘蔗 338 -> 83
        assert_eq!(resolve(&ctx(338, 0, 1, 0.0, 0.5)).unwrap().first.id, 83);
    }

    #[test]
    fn sign_is_standing_on_top_and_wall_on_side() {
        let p = resolve(&ctx(323, 0, 1, 0.0, 0.5)).unwrap();
        assert_eq!(p.first.id, b::SIGN_POST);
        let p = resolve(&ctx(323, 0, 3, 0.0, 0.5)).unwrap();
        assert_eq!((p.first.id, p.first.meta), (b::WALL_SIGN, 3));
    }

    #[test]
    fn door_needs_top_face() {
        assert!(resolve(&ctx(324, 0, 3, 0.0, 0.5)).is_none());
        assert!(resolve(&ctx(324, 0, 1, 0.0, 0.5)).is_some());
    }

    #[test]
    fn stairs_direction_and_upside_down() {
        // 朝北(yaw 0 -> direction 1 西) STAIR_FACE[1] = 2
        let p = resolve(&ctx(53, 0, 1, 0.0, 0.5)).unwrap();
        assert_eq!(p.first.meta, 2);
        // 倒置楼梯 点底面
        let p = resolve(&ctx(53, 0, 0, 0.0, 0.5)).unwrap();
        assert_eq!(p.first.meta, 2 | 0x04);
        // 下半格命中 侧面
        let p = resolve(&ctx(53, 0, 2, 0.0, 0.2)).unwrap();
        assert_eq!(p.first.meta, 2);
        let p = resolve(&ctx(53, 0, 2, 0.0, 0.8)).unwrap();
        assert_eq!(p.first.meta, 2 | 0x04);
    }

    #[test]
    fn log_axis_follows_clicked_face() {
        // 云杉原木 17 damage 1 点顶面 -> 竖着 0
        assert_eq!(resolve(&ctx(17, 1, 1, 0.0, 0.5)).unwrap().first.meta, 1);
        // 点北面 -> z 轴 0b1000
        assert_eq!(resolve(&ctx(17, 1, 2, 0.0, 0.5)).unwrap().first.meta, 1 | 0b1000);
        // 点东面 -> x 轴 0b0100
        assert_eq!(resolve(&ctx(17, 1, 5, 0.0, 0.5)).unwrap().first.meta, 1 | 0b0100);
    }

    #[test]
    fn slab_flips_to_upper_half() {
        assert_eq!(resolve(&ctx(44, 0, 1, 0.0, 0.5)).unwrap().first.meta, 0);
        assert_eq!(resolve(&ctx(44, 0, 0, 0.0, 0.5)).unwrap().first.meta, 0x08);
        assert_eq!(resolve(&ctx(44, 0, 3, 0.0, 0.9)).unwrap().first.meta, 0x08);
        // 木台阶 158 damage 2 白桦
        assert_eq!(resolve(&ctx(158, 2, 1, 0.0, 0.5)).unwrap().first.meta, 2);
    }

    #[test]
    fn non_block_items_place_nothing() {
        assert!(resolve(&ctx(340, 0, 1, 0.0, 0.5)).is_none()); // 书
        assert!(resolve(&ctx(276, 0, 1, 0.0, 0.5)).is_none()); // 钻石剑
        assert!(resolve(&ctx(0, 0, 1, 0.0, 0.5)).is_none()); // 空手
        assert!(resolve(&ctx(300, 0, 9, 0.0, 0.5)).is_none()); // 面越界
    }
}
