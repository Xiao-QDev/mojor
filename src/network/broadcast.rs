use std::net::SocketAddr;

use bytes::Bytes;

use crate::network::protocol::level_event::LevelEventPacket;
use crate::network::protocol::update_block::UpdateBlockPacket;

use super::{NetworkState, SendCmd};

impl NetworkState {
    pub(crate) fn broadcast_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        block_id: u8,
        block_data: u8,
    ) -> Vec<SendCmd> {
        let packet = UpdateBlockPacket::single(x, y, z, block_id, block_data).encode();
        self.viewers_of(x, y, z)
            .map(|peer| reply(peer, vec![packet.clone()]))
            .collect()
    }

    // 方块破坏碎屑 挖的人也要收 0.14 的客户端收到 RemoveBlock 后自己不放这个粒子
    pub(crate) fn broadcast_destroy(
        &self,
        x: i32,
        y: i32,
        z: i32,
        block_id: u8,
        block_data: u8,
    ) -> Vec<SendCmd> {
        let packet = LevelEventPacket::particle_destroy(x, y, z, block_id, block_data).encode();
        self.viewers_of(x, y, z)
            .map(|peer| reply(peer, vec![packet.clone()]))
            .collect()
    }

    // 通用 level event 广播 门开关音效等
    pub(crate) fn broadcast_level_event(
        &self,
        x: i32,
        y: i32,
        z: i32,
        packet: bytes::Bytes,
    ) -> Vec<SendCmd> {
        self.viewers_of(x, y, z)
            .map(|peer| reply(peer, vec![packet.clone()]))
            .collect()
    }

    // 把任意包广播给某个整数格坐标附近的玩家 受击动画等实体事件用
    pub(crate) fn broadcast_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        bodies: Vec<Bytes>,
        skip: Option<std::net::SocketAddr>,
    ) -> Vec<SendCmd> {
        self.viewers_of(x, y, z)
            .filter(|peer| Some(*peer) != skip)
            .map(|peer| reply(peer, bodies.clone()))
            .collect()
    }

    // 看得到这个位置的玩家 视野半径内
    fn viewers_of(&self, x: i32, y: i32, z: i32) -> impl Iterator<Item = SocketAddr> + '_ {
        let radius = (self.view_distance * 16) as f32;
        let radius_sq = radius * radius;
        let center = (x as f32 + 0.5, y as f32, z as f32 + 0.5);
        self.players
            .iter()
            .filter(|(_, state)| state.spawned)
            .filter(move |(_, state)| {
                let dx = state.position.0 - center.0;
                let dy = state.position.1 - center.1;
                let dz = state.position.2 - center.2;
                dx * dx + dy * dy + dz * dz <= radius_sq
            })
            .map(|(peer, _)| *peer)
    }
}

pub(crate) fn reply(addr: SocketAddr, bodies: Vec<Bytes>) -> SendCmd {
    SendCmd::GamePackets { addr, bodies }
}
