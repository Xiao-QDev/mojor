// 下落方块实体 对齐 MCPE 0.14.1 原版 FallingBlock + HeavyBlock
// 沙子/砂砾失去支撑时从方块变成一个自由落体实体, 落到地面后 tryToPlace 变回方块.
// 服务端没有实体碰撞系统, 用"实体底面穿过方块顶面且脚下方块非 free"来近似原版的 onGround.
use bytes::Bytes;

use crate::entity::{ITEM_GRAVITY, ITEM_MAX_FALL_SPEED, PLAYER_HALF_WIDTH};
use crate::level::generator::blocks as b;
use crate::network::protocol::add_entity::{
    AddEntityPacket, ENTITY_TYPE_FALLING_BLOCK, falling_block_metadata,
};
use crate::network::protocol::remove_entity::RemoveEntityPacket;

// 碰撞盒半宽 0.49 与原版 setSize(0.98) 一致
const HALF_WIDTH: f32 = 0.49;
// 半高 原版下落方块近乎满格 用 0.49
const HALF_HEIGHT: f32 = 0.49;
// 超过 100 tick 仍在下落就移除 对齐原版 fallTime > 100
const MAX_FALL_TIME: u32 = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct FallingBlock {
    pub(crate) eid: i64,
    pub(crate) block_id: u8,
    pub(crate) block_data: u8,
    // 实体中心坐标
    pub(crate) position: (f32, f32, f32),
    pub(crate) velocity: (f32, f32, f32),
    pub(crate) fall_time: u32,
    // 从创造模式方块生成 落地时不出掉落物
    pub(crate) creative: bool,
}

// 重力方块 id 对齐原版 HeavyBlock: 沙子 12 砂砾 13 铁砧 166/167
// 0.14 里会下落的就是这几类, 与 Nukkit BlockFallable 一致
pub fn is_fallable(id: u8) -> bool {
    matches!(id, b::SAND | b::GRAVEL | 166 | 167)
}

impl FallingBlock {
    // 从被挖空的那格中心生成 与原版 HeavyBlock::startFalling 一致
    pub(crate) fn new(
        eid: i64,
        block_id: u8,
        block_data: u8,
        block: (i32, i32, i32),
        creative: bool,
    ) -> Self {
        Self {
            eid,
            block_id,
            block_data,
            position: (
                block.0 as f32 + 0.5,
                block.1 as f32 + 0.5,
                block.2 as f32 + 0.5,
            ),
            velocity: (0.0, 0.0, 0.0),
            fall_time: 0,
            creative,
        }
    }

    // 出生包 通用 AddEntity(type=66) + DATA_VARIANT
    pub(crate) fn add_packet(&self) -> Bytes {
        let meta = falling_block_metadata(self.block_id, self.block_data);
        AddEntityPacket {
            eid: self.eid,
            entity_type: ENTITY_TYPE_FALLING_BLOCK,
            position: self.position,
            velocity: self.velocity,
            metadata: &meta,
        }
        .encode()
    }

    pub(crate) fn remove_packet(&self) -> Bytes {
        RemoveEntityPacket { eid: self.eid }.encode()
    }

    // 本 tick 要查的两个方块: 脚下方 与 按当前速度下落后的落点方
    pub(crate) fn probe_columns(&self) -> [(i32, i32, i32); 2] {
        let x = self.position.0.floor() as i32;
        let z = self.position.2.floor() as i32;
        let feet = self.position.1 - HALF_HEIGHT;
        let below = (feet - 0.001).floor() as i32;
        let next_feet = feet + self.velocity.1;
        let ground = next_feet.floor() as i32;
        [(x, below, z), (x, ground, z)]
    }

    // 推进一格物理. below_block/ground_block 由 region 批量查好后传入.
    // 返回落地时方块应固化的格子 (x,y,z), 仍在下落返回 None.
    pub(crate) fn step(&mut self, below_block: u8, ground_block: u8) -> Option<(i32, i32, i32)> {
        self.fall_time += 1;

        // velocity.y -= 0.04; move; *= 0.98  对齐原版 normalTick
        let next_y = self.position.1 + self.velocity.1;
        let next_feet = next_y - HALF_HEIGHT;
        let landing_y = next_feet.floor() as i32;

        // 落地判定: 下落中(vy<=0) 且落点下方不是 free 就停在那个方块顶面
        if self.velocity.1 <= 0.0 && !is_free(ground_block) {
            let top = landing_y + 1;
            self.position.1 = top as f32 + HALF_HEIGHT;
            self.velocity = (0.0, 0.0, 0.0);
            // 方块实体落地后占据它所在的那一格(实体中心向下取整)
            let bx = self.position.0.floor() as i32;
            let by = self.position.1.floor() as i32;
            let bz = self.position.2.floor() as i32;
            return Some((bx, by, bz));
        }

        self.velocity.1 = (self.velocity.1 - ITEM_GRAVITY).max(-ITEM_MAX_FALL_SPEED);
        self.velocity.0 *= 0.98;
        self.velocity.2 *= 0.98;
        self.position.1 = next_y;

        // 下落到基岩以下仍没落地(虚空) 超时移除
        if self.fall_time > MAX_FALL_TIME && below_block == b::AIR {
            return Some((-1, -1, -1)); // 用哨兵告诉调用方"移除但别放方块"
        }
        None
    }

    // 杂中玩家的水平判定 用于砸落伤害
    pub(crate) fn hits_player(&self, player_feet: (f32, f32, f32)) -> bool {
        let reach = HALF_WIDTH + PLAYER_HALF_WIDTH;
        let dx = (self.position.0 - player_feet.0).abs();
        let dz = (self.position.2 - player_feet.2).abs();
        let dy = (self.position.1 - player_feet.1).abs();
        dx <= reach && dz <= reach && dy <= 1.0
    }
}

// 原版 HeavyBlock::isFree: 空气/火/液体(材质 type 0/5/6) 视为自由空间
pub fn is_free(id: u8) -> bool {
    id == b::AIR || crate::level::physics::is_liquid(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallable_ids() {
        assert!(is_fallable(b::SAND));
        assert!(is_fallable(b::GRAVEL));
        assert!(is_fallable(166));
        assert!(!is_fallable(b::STONE));
    }

    #[test]
    fn sand_falls_until_solid_then_lands() {
        let mut f = FallingBlock::new(1, b::SAND, 0, (0, 10, 0), false);
        // 全程下方空气 一直加速下落
        for _ in 0..40 {
            if f.step(b::AIR, b::AIR).is_some() {
                break;
            }
        }
        // 下方出现草(实心) 落点方也是草
        let cell = f.step(b::GRASS, b::GRASS);
        assert!(cell.is_some());
        assert_eq!(f.velocity.1, 0.0);
    }

    #[test]
    fn add_packet_uses_falling_block_type() {
        let f = FallingBlock::new(7, b::SAND, 1, (0, 5, 0), false);
        let data = f.add_packet();
        assert_eq!(data[0], crate::network::protocol::pid::ADD_ENTITY);
        assert_eq!(&data[9..13], &(ENTITY_TYPE_FALLING_BLOCK as i32).to_be_bytes());
        // metadata: 0x54, sand(12)|(1<<8)=268 LE
        assert_eq!(&data[45..51], &[0x54, 0x0c, 0x01, 0x00, 0x00, 0x7f]);
    }
}
