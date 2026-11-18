// 投射物实体 照搬 MCPE 0.14.1 原版 Arrow/Snowball/Egg/EnderPearl
// 服务端做权威抛体物理(重力 0.04 阻尼) 每 tick MoveEntity 同步 撞地后移除
// 命中玩家伤害/传送由 region 检测到后发事件 这里只管实体状态与包
use bytes::Bytes;

use crate::network::protocol::add_entity::AddEntityPacket;
use crate::network::protocol::move_entity::{MoveEntityEntry, MoveEntityPacket};
use crate::network::protocol::remove_entity::RemoveEntityPacket;

// 0.14 实体类型 与 PHP 核心 NETWORK_ID 一致
pub const ENTITY_TYPE_ARROW: i32 = 80;
pub const ENTITY_TYPE_SNOWBALL: i32 = 81;
pub const ENTITY_TYPE_EGG: i32 = 82;
pub const ENTITY_TYPE_ENDER_PEARL: i32 = 87;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectileKind {
    Arrow,
    Snowball,
    Egg,
    EnderPearl,
}

impl ProjectileKind {
    fn entity_type(self) -> i32 {
        match self {
            ProjectileKind::Arrow => ENTITY_TYPE_ARROW,
            ProjectileKind::Snowball => ENTITY_TYPE_SNOWBALL,
            ProjectileKind::Egg => ENTITY_TYPE_EGG,
            ProjectileKind::EnderPearl => ENTITY_TYPE_ENDER_PEARL,
        }
    }
    // 重力系数 原版箭/雪球 0.04 末影珍珠 0.03
    fn gravity(self) -> f32 {
        match self {
            ProjectileKind::EnderPearl => 0.03,
            _ => 0.04,
        }
    }
}

// 物品 legacy id -> 投射物类型 对应原版弓箭/雪球/鸡蛋/末影珍珠
pub fn kind_from_item(id: i16) -> Option<ProjectileKind> {
    match id {
        crate::item::ids::ARROW => Some(ProjectileKind::Arrow),
        crate::item::ids::SNOWBALL => Some(ProjectileKind::Snowball),
        crate::item::ids::EGG => Some(ProjectileKind::Egg),
        crate::item::ids::ENDER_PEARL => Some(ProjectileKind::EnderPearl),
        _ => None,
    }
}

// 由玩家 yaw/pitch(角度) 算发射方向 对齐 Nukkit Player::getDirectionVector
// MCPE 客户端 pitch 正值抬头 所以 y 分量取 sin(pitch)
pub fn direction_from_rotation(yaw: f32, pitch: f32) -> (f32, f32, f32) {
    let yaw = yaw.to_radians();
    let pitch = pitch.to_radians();
    (
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

#[derive(Debug, Clone)]
pub struct Projectile {
    pub eid: i64,
    pub kind: ProjectileKind,
    pub position: (f32, f32, f32),
    pub velocity: (f32, f32, f32),
    pub age: u32,
}

// 原版抛体阻尼 0.99
const DRAG: f32 = 0.99;
// 存活上限 原版箭在地面 5 分钟 这里飞行超时 1000 tick
const MAX_AGE: u32 = 1000;

impl Projectile {
    pub fn new(eid: i64, kind: ProjectileKind, position: (f32, f32, f32), direction: (f32, f32, f32), speed: f32) -> Self {
        let len = (direction.0 * direction.0 + direction.1 * direction.1 + direction.2 * direction.2).sqrt();
        let (dx, dy, dz) = if len > 0.0 {
            (direction.0 / len, direction.1 / len, direction.2 / len)
        } else {
            (0.0, 0.0, 1.0)
        };
        Self {
            eid,
            kind,
            position,
            velocity: (dx * speed, dy * speed, dz * speed),
            age: 0,
        }
    }

    pub fn add_packet(&self) -> Bytes {
        // 投射物元数据用空 blob(只有 0x7f 结尾)
        let meta = [0x7f];
        AddEntityPacket {
            eid: self.eid,
            entity_type: self.kind.entity_type(),
            position: self.position,
            velocity: self.velocity,
            metadata: &meta,
        }
        .encode()
    }

    pub fn move_packet(&self) -> Bytes {
        MoveEntityPacket {
            entries: vec![MoveEntityEntry {
                eid: self.eid,
                position: self.position,
                yaw: 0.0,
                head_yaw: 0.0,
                pitch: 0.0,
            }],
        }
        .encode()
    }

    pub fn remove_packet(&self) -> Bytes {
        RemoveEntityPacket { eid: self.eid }.encode()
    }

    // 推进一拍物理 返回是否撞地/超时需要移除
    // ground_y: 落点脚下第一个非空气方块的顶面 y (由 region 查询传入) None 表示下方全空气
    pub fn step(&mut self, ground_top_y: Option<f32>) -> bool {
        self.age += 1;
        // 重力 + 阻尼 对齐原版
        self.velocity.1 -= self.kind.gravity();
        self.position.0 += self.velocity.0;
        self.position.1 += self.velocity.1;
        self.position.2 += self.velocity.2;
        self.velocity.0 *= DRAG;
        self.velocity.1 *= DRAG;
        self.velocity.2 *= DRAG;

        // 撞地: 实体中心落到地面顶面 + 半径以下
        if let Some(top) = ground_top_y {
            if self.position.1 <= top + 0.1 {
                self.position.1 = top + 0.1;
                return true;
            }
        }
        self.age > MAX_AGE || self.position.1 < 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_entity_type_and_add_packet() {
        let p = Projectile::new(7, ProjectileKind::Arrow, (0.5, 5.0, 0.5), (0.0, 0.0, 1.0), 2.0);
        let data = p.add_packet();
        assert_eq!(data[0], crate::network::protocol::pid::ADD_ENTITY);
        assert_eq!(&data[9..13], &ENTITY_TYPE_ARROW.to_be_bytes());
        assert!((p.velocity.2 - 2.0).abs() < 1e-5);
    }

    #[test]
    fn projectile_falls_and_hits_ground() {
        let mut p = Projectile::new(1, ProjectileKind::Snowball, (0.5, 10.0, 0.5), (0.0, 0.0, 1.0), 1.0);
        // 地面顶面 y=5 一路下落直到撞地
        let mut hit = false;
        for _ in 0..100 {
            if p.step(Some(5.0)) {
                hit = true;
                break;
            }
        }
        assert!(hit, "雪球应撞地");
        assert!(p.position.1 >= 5.0);
    }

    #[test]
    fn ender_pearl_lower_gravity_than_arrow() {
        let arrow = ProjectileKind::Arrow.gravity();
        let pearl = ProjectileKind::EnderPearl.gravity();
        assert!(pearl < arrow);
    }

    #[test]
    fn kind_from_legacy_item_ids() {
        assert_eq!(kind_from_item(crate::item::ids::ARROW), Some(ProjectileKind::Arrow));
        assert_eq!(kind_from_item(crate::item::ids::SNOWBALL), Some(ProjectileKind::Snowball));
        assert_eq!(kind_from_item(crate::item::ids::EGG), Some(ProjectileKind::Egg));
        assert_eq!(kind_from_item(crate::item::ids::ENDER_PEARL), Some(ProjectileKind::EnderPearl));
        assert_eq!(kind_from_item(crate::item::ids::APPLE), None);
    }

    #[test]
    fn direction_follows_yaw_pitch() {
        // yaw=0 朝 +Z pitch=0 水平
        let (x, y, z) = direction_from_rotation(0.0, 0.0);
        assert!(x.abs() < 1e-6);
        assert!(y.abs() < 1e-6);
        assert!((z - 1.0).abs() < 1e-6);
        // pitch 正值抬头 -> y 为正
        let (_, up, _) = direction_from_rotation(0.0, 45.0);
        assert!(up > 0.0, "抬头 pitch 应让 y 为正: {up}");
        // yaw=90 朝 -X
        let (left, _, fz) = direction_from_rotation(90.0, 0.0);
        assert!((left + 1.0).abs() < 1e-6, "yaw=90 应朝 -X: {left}");
        assert!(fz.abs() < 1e-6);
    }
}
