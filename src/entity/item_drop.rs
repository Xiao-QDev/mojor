use bytes::Bytes;

use crate::entity::{
    ITEM_GRAVITY, ITEM_HALF_HEIGHT, ITEM_MAX_FALL_SPEED, ITEM_PICKUP_GROW_HORIZONTAL,
    ITEM_PICKUP_GROW_VERTICAL, PLAYER_HALF_WIDTH, PLAYER_HEIGHT,
};
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::item_entity::AddItemEntityPacket;

// 地面上的掉落物 有重力 可被任意玩家拾取 超时后消失
#[derive(Clone)]
pub struct ItemDrop {
    pub(crate) eid: i64,
    pub(crate) item: ItemStack,
    pub(crate) position: (f32, f32, f32),
    pub(crate) velocity: (f32, f32, f32),
    pub(crate) age: u32,
    pub(crate) on_ground: bool,
}

impl ItemDrop {
    pub(crate) fn new(eid: i64, item: ItemStack, block: (i32, i32, i32)) -> Self {
        Self {
            eid,
            item,
            // 与 Nukkit 一致 从方块中心起步 稍微上抛
            position: (
                block.0 as f32 + 0.5,
                block.1 as f32 + 0.5,
                block.2 as f32 + 0.5,
            ),
            velocity: (0.0, 0.2, 0.0),
            age: 0,
            on_ground: false,
        }
    }

    pub(crate) fn add_packet(&self) -> Bytes {
        AddItemEntityPacket {
            eid: self.eid,
            item: self.item,
            position: self.position,
            velocity: self.velocity,
        }
        .encode()
    }

    // 本 tick 要查的两个方块 脚下方与落点方
    // 先把整批掉落物的坐标收齐一次批量查询 避免每格一次跨线程往返
    pub(crate) fn probe_columns(&self) -> [(i32, i32, i32); 2] {
        let x = self.position.0.floor() as i32;
        let z = self.position.2.floor() as i32;
        let below = (self.position.1 - ITEM_HALF_HEIGHT - 0.001).floor() as i32;
        let next_y = self.position.1 + self.velocity.1;
        let ground = (next_y - ITEM_HALF_HEIGHT).floor() as i32;
        [(x, below, z), (x, ground, z)]
    }

    // 只算竖直方向 水平速度为 0 不需要做水平碰撞
    pub(crate) fn fall(&mut self, below_block: u8, ground_block: u8) {
        // 落地后除非脚下方块被挖空 否则不再参与重力计算
        if self.on_ground {
            if below_block != 0 {
                return;
            }
            self.on_ground = false;
        }

        let next_y = self.position.1 + self.velocity.1;
        let ground = (next_y - ITEM_HALF_HEIGHT).floor() as i32;
        if self.velocity.1 <= 0.0 && ground_block != 0 {
            // 贴在方块顶面停住
            self.position.1 = ground as f32 + 1.0 + ITEM_HALF_HEIGHT;
            self.velocity = (0.0, 0.0, 0.0);
            self.on_ground = true;
            return;
        }

        self.velocity.1 = (self.velocity.1 - ITEM_GRAVITY).max(-ITEM_MAX_FALL_SPEED);
        self.position.1 = next_y;
    }

    // 玩家是否够得着 位置为脚部坐标 与 Nukkit 的碰撞盒外扩判定一致
    pub(crate) fn within_pickup_range(&self, player: (f32, f32, f32)) -> bool {
        let horizontal_reach =
            PLAYER_HALF_WIDTH + ITEM_PICKUP_GROW_HORIZONTAL + ITEM_HALF_HEIGHT;
        let vertical_reach = PLAYER_HEIGHT / 2.0 + ITEM_PICKUP_GROW_VERTICAL + ITEM_HALF_HEIGHT;
        let dx = (self.position.0 - player.0).abs();
        let dy = (self.position.1 - (player.1 + PLAYER_HEIGHT / 2.0)).abs();
        let dz = (self.position.2 - player.2).abs();
        dx <= horizontal_reach && dy <= vertical_reach && dz <= horizontal_reach
    }
}
