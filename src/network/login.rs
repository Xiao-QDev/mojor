use bytes::Bytes;
use tracing::warn;

use crate::network::protocol::adventure_settings::{
    AdventureSettingsPacket, FLAG_ALLOW_FLIGHT, FLAG_AUTO_JUMP, PERMISSION_MEMBER,
};
use crate::item::inventory::CraftingManager;
use crate::network::protocol::container::CraftingDataPacket;
use crate::network::protocol::container_set_content::ContainerSetContentPacket;
use crate::network::protocol::login::LoginPacket;
use crate::network::protocol::move_player::EYE_HEIGHT;
use crate::network::protocol::pid::{ACCEPTED_PROTOCOLS, CURRENT_PROTOCOL};
use crate::network::protocol::play_status::{
    LOGIN_FAILED_CLIENT, LOGIN_FAILED_SERVER, LOGIN_SUCCESS, PLAYER_SPAWN, PlayStatusPacket,
};
use crate::network::protocol::respawn::RespawnPacket;
use crate::network::protocol::set_difficulty::SetDifficultyPacket;
use crate::network::protocol::set_player_gametype::SetPlayerGameTypePacket;
use crate::network::protocol::set_spawn_position::SetSpawnPositionPacket;
use crate::network::protocol::start_game::StartGamePacket;
use crate::network::protocol::update_attributes::UpdateAttributesPacket;

use super::GAMEMODE_CREATIVE;
use crate::level::WorldTime;

pub(crate) fn protocol_accepted(login: &LoginPacket) -> bool {
    // 与 ITX/Genisys Player.php 的 in_array(protocol1, ACCEPTED_PROTOCOLS) 对齐
    // 同时兼容 protocol2 字段 覆盖 0.14.0=45 0.14.1=46 0.14.2=60 0.14.3=70
    ACCEPTED_PROTOCOLS.contains(&login.protocol1) || ACCEPTED_PROTOCOLS.contains(&login.protocol2)
}

// 折算出与客户端握手时使用的协议号 供后续按版本区分包结构(如 UseItem 的 slot 字段)
// Nukkit/Genisys 用 protocol1 作为玩家的 protocol
pub(crate) fn client_protocol(login: &LoginPacket) -> i32 {
    login.protocol1
}

pub(crate) fn login_sequence(
    login: &LoginPacket,
    gamemode: u8,
    difficulty: i32,
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
        // 照搬 ITX/Genisys Player.php: 低于当前协议提示客户端过旧 否则提示服务端过旧
        let status = if login.protocol1 < CURRENT_PROTOCOL {
            LOGIN_FAILED_CLIENT
        } else {
            LOGIN_FAILED_SERVER
        };
        packets.push(PlayStatusPacket { status }.encode());
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
            // spawn 是方块坐标不动 实体 position 的 y 客户端按眼睛位置解释
            spawn: spawn_block,
            position: (spawn_position.0, spawn_position.1 + EYE_HEIGHT, spawn_position.2),
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
    packets.push(SetDifficultyPacket { difficulty }.encode());
    packets.push(
        SetPlayerGameTypePacket {
            gamemode: (gamemode & 0x01) as i32,
        }
        .encode(),
    );
    packets.push(adventure_settings(gamemode).encode());
    // 客户端靠这张表在合成网格里算产物 没下发就完全不能合成
    packets.push(
        CraftingDataPacket { clean_recipes: true }
            .encode(&CraftingManager::new()),
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::login::Skin;

    fn login_with(protocol: i32) -> LoginPacket {
        LoginPacket {
            username: "tester".to_string(),
            protocol1: protocol,
            protocol2: protocol,
            client_id: 0,
            client_uuid: [0; 16],
            server_address: String::new(),
            client_secret: Bytes::new(),
            skin: Skin {
                model: String::new(),
                data: Bytes::new(),
            },
        }
    }

    #[test]
    fn accepts_all_014_protocols() {
        // 0.14.0~0.14.3 四个协议号都要放行 与 Info.php ACCEPTED_PROTOCOLS 一致
        for &p in &ACCEPTED_PROTOCOLS {
            assert!(protocol_accepted(&login_with(p)), "协议 {p} 应被接受");
            assert_eq!(client_protocol(&login_with(p)), p);
        }
    }

    #[test]
    fn rejects_unsupported_protocols() {
        assert!(!protocol_accepted(&login_with(44)));
        assert!(!protocol_accepted(&login_with(80)));
    }

    #[test]
    fn login_uses_current_difficulty_not_hardcoded() {
        // /difficulty 改过的难度要写进新玩家的登录包(索引 5 是 SetDifficulty)
        for value in [0i32, 1, 2, 3] {
            let packets = login_sequence(
                &login_with(70),
                0,
                value,
                0,
                (0, 0, 0),
                (0.0, 0.0, 0.0),
                &WorldTime::default(),
            );
            let body = &packets[5];
            assert_eq!(body[0], crate::network::protocol::pid::SET_DIFFICULTY);
            assert_eq!(&body[1..5], &value.to_be_bytes(), "难度未写入登录包");
        }
    }

    #[test]
    fn protocol2_can_satisfy_acceptance() {
        // 有些客户端只把协议号写进 protocol2
        let mut login = login_with(0);
        login.protocol2 = 70;
        assert!(protocol_accepted(&login));
    }

    #[test]
    fn rejected_client_gets_status_following_version() {
        use crate::network::protocol::play_status::LOGIN_FAILED_SERVER;
        // 过旧客户端 -> LOGIN_FAILED_CLIENT
        let old = login_sequence(&login_with(44), 1, 2, 0, (0, 0, 0), (0.0, 0.0, 0.0), &WorldTime::default());
        assert!(!old.is_empty());
        assert_eq!(old[0][..], [0x90, 0, 0, 0, LOGIN_FAILED_CLIENT as u8]);
        // 过新客户端 -> LOGIN_FAILED_SERVER
        let new = login_sequence(&login_with(80), 1, 2, 0, (0, 0, 0), (0.0, 0.0, 0.0), &WorldTime::default());
        assert!(!new.is_empty());
        assert_eq!(new[0][..], [0x90, 0, 0, 0, LOGIN_FAILED_SERVER as u8]);
    }
}

pub(crate) fn spawn_sequence(position: (f32, f32, f32), gamemode: u8) -> Vec<Bytes> {
    let mut packets = vec![
        // 背包与盔甲槽 生存模式下客户端以此为准
        ContainerSetContentPacket::empty_inventory().encode(),
        ContainerSetContentPacket::empty_armor().encode(),
    ];
    // 创造模式额外下发创造背包(window 0x79) 客户端以此渲染创造物品栏
    // 照搬 Nukkit Player.sendCreativeInventory
    if gamemode & GAMEMODE_CREATIVE != 0 {
        packets.push(
            ContainerSetContentPacket::creative(crate::item::creative::creative_inventory())
                .encode(),
        );
    }
    packets.push(
        RespawnPacket {
            x: position.0,
            // 与 StartGame 一致 客户端把这里的 y 当眼睛位置
            y: position.1 + EYE_HEIGHT,
            z: position.2,
        }
        .encode(),
    );
    packets.push(
        PlayStatusPacket {
            status: PLAYER_SPAWN,
        }
        .encode(),
    );
    packets
}
