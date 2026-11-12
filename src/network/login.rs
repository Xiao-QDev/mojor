use bytes::Bytes;
use tracing::warn;

use crate::network::protocol::adventure_settings::{
    AdventureSettingsPacket, FLAG_ALLOW_FLIGHT, FLAG_AUTO_JUMP, PERMISSION_MEMBER,
};
use crate::network::protocol::container_set_content::ContainerSetContentPacket;
use crate::network::protocol::login::LoginPacket;
use crate::network::protocol::pid::CURRENT_PROTOCOL;
use crate::network::protocol::play_status::{
    LOGIN_FAILED_CLIENT, LOGIN_SUCCESS, PLAYER_SPAWN, PlayStatusPacket,
};
use crate::network::protocol::respawn::RespawnPacket;
use crate::network::protocol::set_difficulty::{NORMAL, SetDifficultyPacket};
use crate::network::protocol::set_player_gametype::SetPlayerGameTypePacket;
use crate::network::protocol::set_spawn_position::SetSpawnPositionPacket;
use crate::network::protocol::start_game::StartGamePacket;
use crate::network::protocol::update_attributes::UpdateAttributesPacket;

use super::GAMEMODE_CREATIVE;
use super::world_time::WorldTime;

pub(crate) fn protocol_accepted(login: &LoginPacket) -> bool {
    login.protocol1 == CURRENT_PROTOCOL || login.protocol2 == CURRENT_PROTOCOL
}

pub(crate) fn login_sequence(
    login: &LoginPacket,
    gamemode: u8,
    seed: u32,
    spawn_block: (i32, i32, i32),
    spawn_position: (f32, f32, f32),
    world_time: &WorldTime,
) -> Vec<Bytes> {
    let mut packets = Vec::new();

    if !protocol_accepted(login) {
        warn!(
            protocol1 = login.protocol1,
            protocol2 = login.protocol2,
            "客户端协议版本不匹配"
        );
        packets.push(
            PlayStatusPacket {
                status: LOGIN_FAILED_CLIENT,
            }
            .encode(),
        );
        return packets;
    }

    packets.push(PlayStatusPacket { status: LOGIN_SUCCESS }.encode());
    packets.push(
        StartGamePacket {
            // 客户端不生成地形 种子只用于它自己的环境计算
            seed: seed as i32,
            dimension: 0,
            generator: 1,
            gamemode: (gamemode & 0x01) as i32,
            // 与 Nukkit 一致 客户端自视角的实体 id 固定为 0
            eid: 0,
            spawn: spawn_block,
            position: spawn_position,
            b1: true,
            b2: true,
            b3: false,
            unknown: String::new(),
        }
        .encode(),
    );
    // 照搬 Nukkit Level.sendTime(player) 停时先补 started=true 再发 started=false
    packets.extend(world_time.time_packets());
    packets.push(
        SetSpawnPositionPacket {
            x: spawn_block.0,
            y: spawn_block.1,
            z: spawn_block.2,
        }
        .encode(),
    );
    // eid 0 固定代表客户端自己的实体
    packets.push(UpdateAttributesPacket::survival(0).encode());
    packets.push(
        SetDifficultyPacket {
            difficulty: NORMAL,
        }
        .encode(),
    );
    packets.push(
        SetPlayerGameTypePacket {
            gamemode: (gamemode & 0x01) as i32,
        }
        .encode(),
    );
    packets.push(adventure_settings(gamemode).encode());
    packets
}

// 生存不给飞行权限 创造才有
pub(crate) fn adventure_settings(gamemode: u8) -> AdventureSettingsPacket {
    let mut flags = FLAG_AUTO_JUMP;
    if gamemode & GAMEMODE_CREATIVE != 0 {
        flags |= FLAG_ALLOW_FLIGHT;
    }
    AdventureSettingsPacket {
        flags,
        user_permission: PERMISSION_MEMBER,
        global_permission: PERMISSION_MEMBER,
    }
}

pub(crate) fn spawn_sequence(position: (f32, f32, f32)) -> Vec<Bytes> {
    vec![
        // 背包与盔甲槽 生存模式下客户端以此为准
        ContainerSetContentPacket::empty_inventory().encode(),
        ContainerSetContentPacket::empty_armor().encode(),
        RespawnPacket {
            x: position.0,
            y: position.1,
            z: position.2,
        }
        .encode(),
        PlayStatusPacket {
            status: PLAYER_SPAWN,
        }
        .encode(),
    ]
}

// 解析失败时打印包头 便于对照客户端实际协议布局
pub(crate) fn hex_head(bytes: &[u8], limit: usize) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(limit * 3);
    for b in bytes.iter().take(limit) {
        let _ = write!(out, "{b:02x} ");
    }
    out.trim_end().to_string()
}
