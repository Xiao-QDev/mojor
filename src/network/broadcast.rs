use std::net::SocketAddr;

use bytes::Bytes;

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
        // 方块改动只给看得到这个位置的人
        let radius = (self.view_distance * 16) as f32;
        let radius_sq = radius * radius;
        let center = (x as f32 + 0.5, y as f32, z as f32 + 0.5);
        self.players
            .iter()
            .filter(|(_, state)| state.spawned)
            .filter(|(_, state)| {
                let dx = state.position.0 - center.0;
                let dy = state.position.1 - center.1;
                let dz = state.position.2 - center.2;
                dx * dx + dy * dy + dz * dz <= radius_sq
            })
            .map(|(peer, _)| reply(*peer, vec![packet.clone()]))
            .collect()
    }

}

pub(crate) fn reply(addr: SocketAddr, bodies: Vec<Bytes>) -> SendCmd {
    SendCmd::GamePackets { addr, bodies }
}
