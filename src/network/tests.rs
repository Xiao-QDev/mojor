use std::time::Duration;

use super::*;
use crate::network::protocol::text::{MAX_MESSAGE_LEN, TYPE_CHAT, TextPacket};
use crate::network::protocol::full_chunk_data::ORDER_COLUMNS;
use crate::network::protocol::adventure_settings::FLAG_ALLOW_FLIGHT;
use crate::network::protocol::pid::{
    ADD_PLAYER, ADVENTURE_SETTINGS, CHUNK_RADIUS_UPDATE, CONTAINER_SET_CONTENT,
    FULL_CHUNK_DATA, MOVE_ENTITY, MOVE_PLAYER, REMOVE_BLOCK, REQUEST_CHUNK_RADIUS, USE_ITEM,
    ADD_ITEM_ENTITY, PLAY_STATUS, PLAYER_LIST, REMOVE_ENTITY, REMOVE_PLAYER, RESPAWN,
    SET_DIFFICULTY, TEXT,
    SET_PLAYER_GAMETYPE, SET_SPAWN_POSITION, SET_TIME, START_GAME, TAKE_ITEM_ENTITY,
    LEVEL_EVENT, UPDATE_ATTRIBUTES, UPDATE_BLOCK, CRAFTING_DATA,
};
use crate::network::protocol::text::TYPE_RAW;
use crate::entity::player::{FLAG_SNEAKING, FLAG_SPRINTING};
use crate::network::protocol::pid::{ANIMATE, PLAYER_ACTION, SET_ENTITY_DATA};
use crate::network::protocol::player_action::{
    ACTION_START_SNEAK, ACTION_START_SPRINT, ACTION_STOP_SNEAK, ACTION_STOP_SPRINT,
};
use crate::network::protocol::animate::ACTION_SWING_ARM;
use crate::level::generator::terrain::SEA_LEVEL;
use crate::level::actor::REGION_TICK;
use crate::utils::binary::BinaryWriter;
use uuid::Uuid;

fn addr() -> SocketAddr {
    "127.0.0.1:50000".parse().unwrap()
}

fn peer(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
}

fn login_packet() -> Vec<u8> {
    login_packet_for("Xiao_QDev", [0; 16])
}

// 与 login_packet_for 写进登录包的 uuid 一致 读存档时按同一个值找
fn login_uuid(byte: u8) -> Uuid {
    Uuid::from_bytes([byte; 16])
}

fn login_packet_for(username: &str, uuid: [u8; 16]) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(LOGIN);
    w.put_string(username).unwrap();
    w.put_i32(70);
    w.put_i32(70);
    w.put_i64(1);
    w.put_bytes(&uuid);
    w.put_string("127.0.0.1:19132").unwrap();
    w.put_string("secret").unwrap();
    w.put_string("Standard_Steve").unwrap();
    w.put_u16(4);
    w.put_bytes(&[0x7f; 4]);
    w.freeze().to_vec()
}

fn request_radius(radius: i32) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(REQUEST_CHUNK_RADIUS);
    w.put_i32(radius);
    w.freeze().to_vec()
}

fn move_player(x: f32, y: f32, z: f32) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(MOVE_PLAYER);
    w.put_i64(0);
    w.put_f32(x);
    w.put_f32(y);
    w.put_f32(z);
    w.put_f32(0.0);
    w.put_f32(0.0);
    w.put_f32(0.0);
    w.put_u8(0);
    w.put_u8(1);
    w.freeze().to_vec()
}

fn remove_block(x: i32, y: i32, z: i32) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(REMOVE_BLOCK);
    w.put_i64(0);
    w.put_i32(x);
    w.put_i32(z);
    w.put_u8(y as u8);
    w.freeze().to_vec()
}

fn use_item(x: i32, y: i32, z: i32, face: u8, item_id: i16) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(USE_ITEM);
    w.put_i32(x);
    w.put_i32(y);
    w.put_i32(z);
    w.put_u8(face);
    for _ in 0..6 {
        w.put_f32(0.0);
    }
    w.put_i32(0);
    if item_id <= 0 {
        w.put_u16(0);
    } else {
        w.put_u16(item_id as u16);
        w.put_u8(1);
        w.put_u16(0);
        w.put_u16(0);
    }
    w.freeze().to_vec()
}

fn chat_packet(message: &str) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(TEXT);
    w.put_u8(TYPE_CHAT);
    // 客户端自带的来源不可信 服务端应忽略并用自己的用户名
    w.put_string("forged").unwrap();
    w.put_string(message).unwrap();
    w.freeze().to_vec()
}

// 空闲窗口 2s 会跨过世界时间的 200 tick 同步周期
// 周期性的 SetTime 是背景噪声 不该混进"这次操作发了什么包"的断言里
// 关心时间同步的用例直接用 bodies_of 看原始包
fn without_periodic(ids: impl Iterator<Item = u8>) -> Vec<u8> {
    ids.filter(|id| *id != SET_TIME).collect()
}

fn packet_ids(commands: &[SendCmd]) -> Vec<u8> {
    without_periodic(
        commands
            .iter()
            .flat_map(|cmd| bodies(cmd).iter().map(|body| body[0]).collect::<Vec<_>>()),
    )
}

fn packet_ids_of(commands: &[SendCmd], target: SocketAddr) -> Vec<u8> {
    without_periodic(bodies_of(commands, target).iter().map(|body| body[0]))
}

fn bodies_of(commands: &[SendCmd], target: SocketAddr) -> Vec<Bytes> {
    commands
        .iter()
        .filter_map(|cmd| match cmd {
            SendCmd::GamePackets { addr, bodies } if *addr == target => Some(bodies.to_vec()),
            _ => None,
        })
        .flatten()
        .collect()
}

async fn spawn_two(gamemode: u8) -> (NetworkState, SocketAddr, SocketAddr) {
    let mut state = NetworkState::new(gamemode);
    let alice = peer(50001);
    let bob = peer(50002);
    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));
    drain_by_peer(&mut state).await;
    (state, alice, bob)
}

// 挖不动的时候(基岩) 客户端可能已经把这格本地删了 只给他本人补一条方块更新
#[tokio::test]
async fn refused_break_restores_block_for_breaker_only() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_CREATIVE).await;
    let mut out = Vec::new();
    state.on_region_event(
        crate::level::actor::RegionEvent::BlockBroken {
            addr: alice,
            x: 0,
            y: 1,
            z: 0,
            id: crate::level::generator::blocks::BEDROCK,
            meta: 0,
            ok: false,
            paired: None,
        },
        &mut out,
    );

    // 只有挖的人收包 旁观者不该看到任何变化
    assert_eq!(packet_ids(&out), vec![UPDATE_BLOCK]);
    assert!(bodies_of(&out, bob).is_empty());
    let bodies = bodies_of(&out, alice);
    assert_eq!(bodies.len(), 1);
    // 0 pid | 1..5 count | 5..9 x | 9..13 z | 13 y | 14 id | 15 flags|meta
    assert_eq!(bodies[0][14], crate::level::generator::blocks::BEDROCK);
}

// 拆门的两半 两格都要广播空气与碎屑 旁观者也看得见
#[tokio::test]
async fn paired_door_break_broadcasts_both_cells() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_CREATIVE).await;
    let mut out = Vec::new();
    state.on_region_event(
        crate::level::actor::RegionEvent::BlockBroken {
            addr: alice,
            x: 0,
            y: 5,
            z: 0,
            id: crate::level::generator::blocks::WOOD_DOOR_BLOCK,
            meta: 0,
            ok: true,
            paired: Some((0, 6, 0, crate::level::generator::blocks::WOOD_DOOR_BLOCK, 0x08)),
        },
        &mut out,
    );

    // 两个已出生玩家 每格各收一条方块更新与碎屑
    assert_eq!(
        packet_ids(&out),
        vec![
            UPDATE_BLOCK, UPDATE_BLOCK, UPDATE_BLOCK, UPDATE_BLOCK, LEVEL_EVENT, LEVEL_EVENT,
            LEVEL_EVENT, LEVEL_EVENT
        ]
    );
    assert_eq!(bodies_of(&out, alice).len(), 4);
    assert_eq!(bodies_of(&out, bob).len(), 4);
}

// 开关门 新 meta 和音效都要广播给视野内的人 旁观者也看得见
#[tokio::test]
async fn toggling_door_broadcasts_state_and_sound() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_CREATIVE).await;
    let mut out = Vec::new();
    state.on_region_event(
        crate::level::actor::RegionEvent::DoorToggled {
            addr: alice,
            x: 0,
            y: 5,
            z: 0,
            id: crate::level::generator::blocks::WOOD_DOOR_BLOCK,
            meta: 0x04,
        },
        &mut out,
    );

    // 先全员方块更新 再全员门音效 每人各一条
    assert_eq!(
        packet_ids(&out),
        vec![UPDATE_BLOCK, UPDATE_BLOCK, LEVEL_EVENT, LEVEL_EVENT]
    );
    assert_eq!(bodies_of(&out, alice).len(), 2);
    assert_eq!(bodies_of(&out, bob).len(), 2);
    // 音效的 evid 是 1003 小端写在 1..3
    let sound = bodies_of(&out, alice)
        .into_iter()
        .find(|b| b[0] == LEVEL_EVENT)
        .unwrap();
    assert_eq!(&sound[1..3], &1003i16.to_be_bytes());
}

// 挖方块要改服务端世界 并在原位生成掉落物
#[tokio::test]
async fn digging_updates_world_and_spawns_drop() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    assert_eq!(state.block_at(0, 4, 0), crate::level::generator::flat::GRASS);

    // 挖掘交给属主分片去做 结果以事件回来
    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    // 两个人都先挪开 长窗口里掉落物会被脚底下的玩家捡走
    state.players.get_mut(&alice).unwrap().position = (0.5, 5.0, 8.5);
    state.players.get_mut(&bob).unwrap().position = (20.5, 5.0, 20.5);
    // 掉落物一出现就收手 再等物理就把它推走了
    let commands = pump_until(&mut state, |ids| {
        ids.iter().filter(|id| **id == ADD_ITEM_ENTITY).count() >= 2
    })
    .await;
    assert_eq!(state.block_at(0, 4, 0), 0);
    // 两个已出生玩家各收方块更新与破坏碎屑 再各收一份掉落物
    assert_eq!(
        packet_ids(&commands),
        vec![
            UPDATE_BLOCK,
            UPDATE_BLOCK,
            LEVEL_EVENT,
            LEVEL_EVENT,
            ADD_ITEM_ENTITY,
            ADD_ITEM_ENTITY
        ]
    );
    // 掉落物镜像是每 tick 的快照 可能比实体包晚一拍
    wait_for_drop(&mut state).await;
    assert_eq!(state.drops.len(), 1);
    assert!(state.players[&alice].inventory[0].is_none());
}

// 改过的方块落盘后重新打开还在 没改过的部分仍是平坦地形
#[tokio::test]
async fn edited_block_survives_reload() {
    let path = std::env::temp_dir().join(format!(
        "violin-net-persist-{}-{}.redb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        let alice = peer(50003);
        state.handle_game_packet(alice, &login_packet_for("alice", [0x33; 16]));
        drain_by_peer(&mut state).await;

        state.handle_game_packet(alice, &remove_block(0, 4, 0));
        // 挖掘在分片线程里完成 等结果回来再落盘
        drain_by_peer(&mut state).await;
        assert_eq!(state.block_at(0, 4, 0), 0);
        // 视野内新生成的区块也会一并落盘(避免重生成丢改动) 所以只要求至少写了被挖的区块
        assert!(state.flush_world().unwrap() >= 1);
    }

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        assert_eq!(state.block_at(0, 4, 0), 0);
        assert_eq!(state.block_at(0, 3, 0), crate::level::generator::flat::DIRT);
        assert_eq!(state.flush_world().unwrap(), 0);
    }

    let _ = std::fs::remove_file(&path);
}

// 区块刚生成、光照还没回执就退出 编辑也不能丢
// 分片停机必须先 drain 在途光照 否则 lit=false 的区块会被 flush 跳过
#[tokio::test]
async fn edit_before_light_done_survives_logout() {
    let path = std::env::temp_dir().join(format!(
        "violin-net-drain-{}-{}.redb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        let alice = peer(50005);
        state.handle_game_packet(alice, &login_packet_for("alice", [0x55; 16]));
        drain_by_peer(&mut state).await;

        // 走远 让属主现场生成一批还没照亮的新区块
        state.handle_game_packet(alice, &move_player(10.5, 5.0, 40.5));
        // 新区块的方块可以挖 只校验距离 不要求已经 lit
        state.handle_game_packet(alice, &remove_block(10, 4, 40));
        // 不 drain：光照回执还没回来就撤分片 停机路径要先 drain 再落盘
        state.on_session_closed(0, &alice, "Disconnected");
    }

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        assert_eq!(state.block_at(10, 4, 40), 0, "停机前没等到光照 编辑丢了");
    }

    let _ = std::fs::remove_file(&path);
}

// 掉落物从方块中心上抛再落地 站在上层够不到 掉进坑里才捡得到
#[tokio::test]
async fn drop_lands_then_is_picked_up() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    // bob 走远 保证只有 alice 在附近
    state.handle_game_packet(bob, &move_player(20.5, 5.0, 20.5));
    drain_by_peer(&mut state).await;

    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    // 先挪开 避免掉落物一出现就被捡走
    state.players.get_mut(&alice).unwrap().position = (0.5, 5.0, 8.5);
    // 挖掘在分片线程里完成 掉落物一出现就读 等长窗口物理已经落地了
    wait_for_drop(&mut state).await;
    // 与 Nukkit 一致 掉落物从方块中心起步 这时已经被物理推上去一点
    let (x, y, z) = state.drops[0].position;
    assert_eq!((x, z), (0.5, 0.5));
    assert!((4.5..=5.0).contains(&y), "掉落物高度不对: {y}");

    advance_ticks(&mut state, (ITEM_PICKUP_DELAY + 5) as usize).await;
    // 落到下方石头顶面 速度归零
    assert_eq!(state.drops.len(), 1);
    assert_eq!(state.drops[0].position.1, 4.0 + ITEM_HALF_HEIGHT);
    assert_eq!(state.drops[0].velocity, (0.0, 0.0, 0.0));

    // 站回坑边 垂直方向差太远捡不到
    state.players.get_mut(&alice).unwrap().position = (0.5, 5.0, 0.5);
    advance_ticks(&mut state, 2).await;
    assert_eq!(state.drops.len(), 1);

    // 掉进坑里后立刻捡起
    state.players.get_mut(&alice).unwrap().position = (0.5, 4.0, 0.5);
    let commands = advance_ticks(&mut state, 3).await;
    assert!(state.drops.is_empty());

    // 自己收到的拾取包 collector 写 0 之后是移除实体与背包刷新
    assert_eq!(
        packet_ids_of(&commands, alice),
        vec![TAKE_ITEM_ENTITY, REMOVE_ENTITY, CONTAINER_SET_CONTENT]
    );
    // 其他玩家看到的 collector 是拾取者 eid
    assert_eq!(
        packet_ids_of(&commands, bob),
        vec![TAKE_ITEM_ENTITY, REMOVE_ENTITY]
    );

    let alice_bodies = bodies_of(&commands, alice);
    assert_eq!(&alice_bodies[0][9..17], &0i64.to_be_bytes());
    assert_eq!(
        &bodies_of(&commands, bob)[0][9..17],
        &state.players[&alice].eid.to_be_bytes()
    );

    // 草方块进了背包第一格
    let content = &alice_bodies[2];
    assert_eq!(&content[4..6], &2u16.to_be_bytes());
    assert_eq!(content[6], 1);
}

// 掉落物没人捡时超过寿命自动消失
#[tokio::test]
async fn drop_expires_after_lifetime() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    // 两个玩家都走远 掉落物留在原地
    state.handle_game_packet(alice, &move_player(20.5, 5.0, 20.5));
    state.handle_game_packet(bob, &move_player(20.5, 5.0, 20.5));
    drain_by_peer(&mut state).await;

    state.set_drop_age_for_test(ITEM_LIFETIME);
    let commands = pump(&mut state).await;
    assert!(state.drops.is_empty());
    assert_eq!(packet_ids(&commands), vec![REMOVE_ENTITY, REMOVE_ENTITY]);
}

// 创造模式挖方块直接消失 没有掉落物也不进背包
#[tokio::test]
async fn creative_digging_has_no_drop() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_CREATIVE).await;

    // 挖掘交给属主分片 广播在结果回来时才发
    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    let commands = pump(&mut state).await;
    assert_eq!(state.block_at(0, 4, 0), 0);
    // 每个已出生玩家收方块更新与破坏碎屑 创造不掉落
    assert_eq!(
        packet_ids(&commands),
        vec![UPDATE_BLOCK, UPDATE_BLOCK, LEVEL_EVENT, LEVEL_EVENT]
    );
}

// 聊天广播给所有已出生玩家 来源以服务端记录的用户名为准
#[tokio::test]
async fn chat_is_broadcast_with_server_side_name() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("hello"));

    assert_eq!(packet_ids(&commands), vec![TEXT, TEXT]);
    for peer in [alice, bob] {
        let bodies = bodies_of(&commands, peer);
        assert_eq!(bodies.len(), 1);
        let text = TextPacket::decode(&bodies[0][..]).unwrap();
        assert_eq!(text.kind, TYPE_CHAT);
        assert_eq!(text.source, "alice");
        assert_eq!(text.message, "hello");
    }
}

// 多行消息按行拆开发送
#[tokio::test]
async fn chat_splits_lines() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("one\ntwo"));
    assert_eq!(packet_ids(&commands), vec![TEXT, TEXT, TEXT, TEXT]);
}

#[tokio::test]
async fn blank_or_oversized_chat_is_dropped() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_SURVIVAL).await;
    assert!(state.handle_game_packet(alice, &chat_packet("   ")).is_empty());

    let too_long = "x".repeat(MAX_MESSAGE_LEN + 1);
    assert!(state.handle_game_packet(alice, &chat_packet(&too_long)).is_empty());
}

#[tokio::test]
async fn chat_before_spawn_is_ignored() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    assert!(state.handle_game_packet(addr(), &chat_packet("hi")).is_empty());
}

#[tokio::test]
async fn placing_block_targets_clicked_face() {
    // 创造模式不扣物品 才能单独验证放置位置
    let (mut state, alice, _) = spawn_two(GAMEMODE_CREATIVE).await;
    // 点击 (2,4,2) 的上表面 石头放到 (2,5,2) 避开玩家身体
    state.handle_game_packet(alice, &use_item(2, 4, 2, 1, 1));
    // 放置交给属分片去做 等结果回来
    drain_by_peer(&mut state).await;
    assert_eq!(state.block_at(2, 5, 2), 1);
}

// 固体方块不能放进自己身体里 站在 (0.5,5,0.5) 往脚下放会被拒
#[tokio::test]
async fn placing_into_own_body_is_rejected() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_CREATIVE).await;
    // 点 (0,4,0) 上表面 目标 (0,5,0) 正是玩家站着的那一格
    let commands = state.handle_game_packet(alice, &use_item(0, 4, 0, 1, 1));

    assert_eq!(state.block_at(0, 5, 0), 0);
    // 客户端已经在本地摆上了 只回一条空气把它撤掉
    assert_eq!(
        without_periodic(packet_ids(&commands).into_iter()),
        vec![UPDATE_BLOCK]
    );
}

// 生存模式背包里没有方块就不能放
#[tokio::test]
async fn placing_without_item_in_inventory_is_rejected() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_SURVIVAL).await;
    state.handle_game_packet(alice, &use_item(0, 4, 0, 1, 1));
    drain_by_peer(&mut state).await;
    assert_eq!(state.block_at(0, 5, 0), 0);
}

// 生存模式放置要从背包扣掉一块
#[tokio::test]
async fn placing_consumes_item_from_inventory() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    // bob 走远 掉落物归 alice
    state.handle_game_packet(bob, &move_player(20.5, 5.0, 20.5));
    drain_by_peer(&mut state).await;

    // 挖掉草方块 等掉落物落到坑底 玩家掉下去后才够得着
    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    state.players.get_mut(&alice).unwrap().position = (0.5, 5.0, 8.5);
    // 挖掘在分片线程里完成 等掉落物出现
    drain_by_peer(&mut state).await;
    advance_ticks(&mut state, (ITEM_PICKUP_DELAY + 5) as usize).await;
    assert!(state.players[&alice].inventory[0].is_none());
    state.players.get_mut(&alice).unwrap().position = (0.5, 4.0, 0.5);
    advance_ticks(&mut state, 3).await;
    assert_eq!(
        state.players[&alice].inventory[0].map(|item| item.id),
        Some(2)
    );

    // 走开一点 否则是往自己脚下放 会被身体碰撞挡住
    // 用移动包而不是直接改字段 分片里的位置才会跟着变
    state.handle_game_packet(alice, &move_player(0.5, 5.0, 3.5));
    drain_by_peer(&mut state).await;
    // 点 (0,3,0) 上表面 把草放回 (0,4,0)
    let commands = state.handle_game_packet(alice, &use_item(0, 3, 0, 1, 2));
    // 背包在投递时就扣掉了 刷新包也在那时发出
    let content = commands
        .iter()
        .flat_map(|cmd| bodies(cmd).to_vec())
        .find(|body| body[0] == CONTAINER_SET_CONTENT)
        .expect("放置后要刷新背包");
    // 草已用完 第一格变空
    assert_eq!(&content[4..6], &[0, 0]);

    // 放置本身在分片线程里完成
    drain_by_peer(&mut state).await;
    assert_eq!(state.block_at(0, 4, 0), 2);
}

#[tokio::test]
async fn empty_hand_and_distant_blocks_are_rejected() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_SURVIVAL).await;

    // 空手不放置
    state.handle_game_packet(alice, &use_item(0, 4, 0, 1, 0));
    assert_eq!(state.block_at(0, 5, 0), 0);

    // 距离过远的挖掘忽略
    let commands = state.handle_game_packet(alice, &remove_block(100, 4, 0));
    assert!(commands.is_empty());
    assert_eq!(state.block_at(100, 4, 0), crate::level::generator::flat::GRASS);
}

fn bodies(cmd: &SendCmd) -> &[Bytes] {
    match cmd {
        SendCmd::GamePackets { bodies, .. } => bodies,
        SendCmd::SessionReady { .. } | SendCmd::Console(_) => &[],
    }
}

// 登录会先发 SessionReady 再发回包 取第一条游戏包序列
fn game_bodies(commands: &[SendCmd]) -> &[Bytes] {
    commands
        .iter()
        .find_map(|cmd| match cmd {
            SendCmd::GamePackets { bodies, .. } => Some(bodies.as_slice()),
            _ => None,
        })
        .expect("应有下行游戏包")
}

#[tokio::test]
async fn login_returns_initial_sequence() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let commands = state.handle_game_packet(addr(), &login_packet());
    // SessionReady + 登录回包
    assert_eq!(commands.len(), 2);

    let packets = game_bodies(&commands);
    // 9 个登录包 末尾是配方表 半径更新照 Nukkit 等客户端请求再回
    assert_eq!(packets.len(), 9);
    assert_eq!(packets[0][0], PLAY_STATUS);
    assert_eq!(packets[1][0], START_GAME);
    assert_eq!(packets[2][0], SET_TIME);
    assert_eq!(packets[3][0], SET_SPAWN_POSITION);
    assert_eq!(packets[4][0], UPDATE_ATTRIBUTES);
    assert_eq!(packets[5][0], SET_DIFFICULTY);
    assert_eq!(packets[6][0], SET_PLAYER_GAMETYPE);
    assert_eq!(packets[7][0], ADVENTURE_SETTINGS);
    assert_eq!(packets[8][0], CRAFTING_DATA);

    // 默认生存：StartGame 的 gamemode 字段为 0 且不给飞行权限
    assert_eq!(i32::from_be_bytes(packets[1][10..14].try_into().unwrap()), 0);
    assert_eq!(&packets[6][1..5], &0i32.to_be_bytes());
    let flags = i32::from_be_bytes(packets[7][1..5].try_into().unwrap());
    assert_eq!(flags & FLAG_ALLOW_FLIGHT, 0);
}

#[tokio::test]
async fn creative_mode_allows_flight() {
    let mut state = NetworkState::new(GAMEMODE_CREATIVE);
    let commands = state.handle_game_packet(addr(), &login_packet());
    let packets = game_bodies(&commands);

    assert_eq!(i32::from_be_bytes(packets[1][10..14].try_into().unwrap()), 1);
    assert_eq!(&packets[6][1..5], &1i32.to_be_bytes());
    let flags = i32::from_be_bytes(packets[7][1..5].try_into().unwrap());
    assert_ne!(flags & FLAG_ALLOW_FLIGHT, 0);
}

#[tokio::test]
async fn repeated_login_is_ignored() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    // SessionReady + 登录回包
    assert_eq!(state.handle_game_packet(addr(), &login_packet()).len(), 2);
    assert!(state.handle_game_packet(addr(), &login_packet()).is_empty());
}

// 区块由 region 线程按自己的周期推进 连续这么多轮空 tick 才认为真的没活了
// 空 bodies 的回执会被网络层丢掉 所以只能靠静默超时判断
// 窗口 2s：光照 worker 在 CPU 争抢时回执间隔可能远超 600ms，宁可多等
// 有产出就重置 所以只影响真的卡住的情形
const IDLE_POLLS: usize = 100;
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(20);

// 反复 tick 直到 region 线程连续几轮没有产出 期间的下行全部收下
async fn pump(state: &mut NetworkState) -> Vec<SendCmd> {
    let mut out = Vec::new();
    let mut idle = 0;
    while idle < IDLE_POLLS {
        let commands = state.tick().await;
        if commands.is_empty() {
            idle += 1;
            tokio::time::sleep(IDLE_POLL_INTERVAL).await;
        } else {
            idle = 0;
            out.extend(commands);
        }
    }
    out
}

// 掉落物一出现就读 不能等 pump 的长窗口：
// 12 个 region tick 足够物理把它推到地面 初相位断言就失效了
async fn wait_for_drop(state: &mut NetworkState) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while state.drops.is_empty() {
        state.tick().await;
        assert!(std::time::Instant::now() < deadline, "掉落物没有出现");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

// 玩家存档是后台线程落盘 并发跑测试时可能晚几拍 轮询等它出现
async fn wait_player_data(dir: &std::path::Path, uuid: Uuid, name: &str) -> PlayerData {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(data) = PlayerData::load(dir, uuid, name) {
            return data;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "玩家存档一直没落盘：{name}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// 收到想要的包就收手 不等空闲窗口
// 窗口拉长之后掉落物可能已经被捡走 再多等只会把后续拾取包也收进来
async fn pump_until(state: &mut NetworkState, want: impl Fn(&[u8]) -> bool) -> Vec<SendCmd> {
    let mut out = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let commands = state.tick().await;
        if !commands.is_empty() {
            out.extend(commands);
            let ids = packet_ids(&out);
            if want(&ids) {
                return out;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "等不到期望的包：{:?}",
            packet_ids(&out)
        );
        tokio::time::sleep(IDLE_POLL_INTERVAL).await;
    }
}

// 实体 tick 在 region 线程按自己的周期跑 测试要按真实时间推进
async fn advance_ticks(state: &mut NetworkState, ticks: usize) -> Vec<SendCmd> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        out.extend(state.tick().await);
        tokio::time::sleep(REGION_TICK).await;
    }
    out.extend(state.tick().await);
    out
}

// 连续 tick 直到没有下行 取出区块包体与其他包 id
async fn drain(state: &mut NetworkState) -> (Vec<Bytes>, Vec<u8>) {
    let mut chunks = Vec::new();
    let mut others = Vec::new();
    for cmd in &pump(state).await {
        for packet in bodies(cmd) {
            if packet[0] == FULL_CHUNK_DATA {
                chunks.push(packet.clone());
            } else if packet[0] != SET_TIME {
                // 长空闲窗口会撞上世界时间同步 不是这批断言关心的
                others.push(packet[0]);
            }
        }
    }
    (chunks, others)
}

fn chunk_coord(packet: &Bytes) -> (i32, i32) {
    (
        i32::from_be_bytes([packet[1], packet[2], packet[3], packet[4]]),
        i32::from_be_bytes([packet[5], packet[6], packet[7], packet[8]]),
    )
}

// 所有玩家退出后 玩家状态与分片都应该被清空
#[tokio::test]
async fn everything_is_released_after_all_players_leave() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.set_idle_ticks_for_test(2);
    state.handle_game_packet(addr(), &login_packet());
    drain_by_peer(&mut state).await;
    assert!(state.level.region_count() > 0);

    state.on_session_closed(0, &addr(), "Disconnected");
    // region 线程异步 按真实时间推进 轮询等到彻底清空 而不是数固定 tick
    // 避免 CPU 争抢时固定窗口不够导致偶发失败
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while (state.level.region_count() > 0 || !state.drops_by_region.is_empty())
        && std::time::Instant::now() < deadline
    {
        advance_ticks(&mut state, 1).await;
    }

    assert!(state.players.is_empty());
    assert!(state.player_index.is_empty());
    assert_eq!(state.level.region_count(), 0, "还有分片没退出");
    assert!(state.drops.is_empty());
    assert!(state.drops_by_region.is_empty(), "掉落物镜像没清干净");
}

// 跨分片迁移走两段式投递 目标分片接管玩家并更新归属表
#[tokio::test]
async fn crossing_region_border_transfers_player() {
    use crate::level::region::RegionId;

    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());
    drain_by_peer(&mut state).await;
    assert_eq!(state.player_index[&addr()], RegionId { x: 0, z: 0 });

    // 512 是分片边界 走到右侧
    state.handle_game_packet(addr(), &move_player(513.5, 5.0, 0.5));
    pump(&mut state).await;

    assert_eq!(state.player_index[&addr()], RegionId { x: 1, z: 0 });
    assert_eq!(state.players[&addr()].position.0, 513.5);
    assert!(state.level.has_region(RegionId { x: 1, z: 0 }));

    // 接管之后包仍能正常处理
    let commands = state.handle_game_packet(addr(), &chat_packet("still here"));
    assert_eq!(packet_ids(&commands), vec![TEXT]);
}

// 玩家视野跨到的分片即使本地没有玩家 也不能被 GC 撤掉
#[tokio::test]
async fn owner_region_referenced_by_remote_player_is_kept() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.set_idle_ticks_for_test(1);
    state.handle_game_packet(addr(), &login_packet());
    drain_by_peer(&mut state).await;

    // 出生点 (0,0) 的视野跨到负坐标 这些分片本地没有玩家
    let neighbour = RegionId { x: -1, z: 0 };
    assert!(state.level.has_region(neighbour));
    advance_ticks(&mut state, 5).await;
    assert!(
        state.level.has_region(neighbour),
        "被别的玩家视野引用的分片被误撤了"
    );
}

// 空闲分片上报后由 global region 撤掉 再用到时按需重建
#[tokio::test]
async fn idle_region_stops_and_is_recreated_on_demand() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.set_idle_ticks_for_test(1);
    // 远处分片没有玩家也没有实体
    assert_eq!(
        state.block_at(600, 4, 0),
        crate::level::generator::flat::GRASS
    );
    assert_eq!(state.level.region_count(), 1);

    pump(&mut state).await;
    assert_eq!(state.level.region_count(), 0, "空闲分片没被撤掉");

    assert_eq!(
        state.block_at(600, 4, 0),
        crate::level::generator::flat::GRASS
    );
    assert_eq!(state.level.region_count(), 1, "分片没有按需重建");
}

// 空闲分片撤销前要把脏块落盘 否则丢改动
#[tokio::test]
async fn idle_region_saves_dirty_chunks_before_stopping() {
    let path = std::env::temp_dir().join(format!(
        "violin-idle-save-{}-{}.redb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        state.set_idle_ticks_for_test(1);
        assert_eq!(
            state.block_at(600, 4, 0),
            crate::level::generator::flat::GRASS
        );
        // 点击草方块上表面 在 (600,5,0) 放一块石头
        state
            .level
            .place_block_for(addr(), (600, 4, 0), (600, 5, 0), 1);
        pump(&mut state).await;
        assert_eq!(state.level.region_count(), 0, "空闲分片没被撤掉");
    }

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        assert_eq!(state.block_at(600, 5, 0), 1, "撤销前没保存脏块");
    }

    let _ = std::fs::remove_file(&path);
}

// 有玩家归属的分片不会被空闲回收
#[tokio::test]
async fn region_with_players_is_not_stopped() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.set_idle_ticks_for_test(1);
    state.handle_game_packet(addr(), &login_packet());
    drain_by_peer(&mut state).await;

    let player_region = state.player_index[&addr()];
    assert!(state.level.has_region(player_region));
    pump(&mut state).await;
    assert!(
        state.level.has_region(player_region),
        "有玩家的分片被撤掉了"
    );
}

// 视距可配 半径 3 时登录要排圆盘里的区块 下发的半径也跟着变
#[tokio::test]
async fn view_distance_controls_radius_and_queue() {
    let mut state = NetworkState::with_world(
        GAMEMODE_SURVIVAL,
        Level::new(),
        0,
        3,
        "world".to_string(),
    );
    let alice = peer(50021);
    let commands = state.handle_game_packet(alice, &login_packet());

    // 登录批次里不再带半径更新 由客户端请求后按配置回
    assert!(
        !commands
            .iter()
            .flat_map(bodies)
            .any(|packet| packet[0] == CHUNK_RADIUS_UPDATE),
        "登录批次不该带半径更新"
    );

    let (chunks, _) = drain(&mut state).await;
    // 半径 3 时下发圈还是完整 7x7 49 块 buffer 要更大视距才切角
    assert_eq!(chunks.len(), 49);

    let commands = state.handle_game_packet(alice, &request_radius(3));
    let packets = bodies(&commands[0]);
    let reply = packets
        .iter()
        .find(|packet| packet[0] == CHUNK_RADIUS_UPDATE)
        .expect("请求后应回半径");
    assert_eq!(&reply[1..5], &3i32.to_be_bytes());
}

// 客户端要求更大的视距时按服务端配置截断
#[tokio::test]
async fn client_radius_is_capped_by_config() {
    let mut state = NetworkState::with_world(
        GAMEMODE_SURVIVAL,
        Level::new(),
        0,
        2,
        "world".to_string(),
    );
    let alice = peer(50022);
    state.handle_game_packet(alice, &login_packet());
    drain(&mut state).await;

    let commands = state.handle_game_packet(alice, &request_radius(8));
    let packets = bodies(&commands[0]);
    let reply = packets
        .iter()
        .find(|packet| packet[0] == CHUNK_RADIUS_UPDATE)
        .expect("应回复半径");
    assert_eq!(&reply[1..5], &2i32.to_be_bytes());
}

// 不依赖客户端请求 登录后由 tick 分批下发视野圈 最后补出生状态
#[tokio::test]
async fn login_queues_chunks_and_spawn_over_ticks() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

    let (chunks, tail) = drain(&mut state).await;
    // 半径 2 时 buffer 内缩后角上仍含 下发圈是完整 5x5
    assert_eq!(chunks.len(), 25);
    // 列表项必须最先到 紧跟进服广播(自己也收到) 之后才是背包 重生与出生状态
    assert_eq!(
        tail,
        vec![
            PLAYER_LIST,
            TEXT,
            CONTAINER_SET_CONTENT,
            CONTAINER_SET_CONTENT,
            RESPAWN,
            PLAY_STATUS
        ]
    );

    // 跨界区块由各自的属主分片生成 下发顺序不固定 只看集合
    let mut coords: Vec<(i32, i32)> = chunks.iter().map(chunk_coord).collect();
    coords.sort_unstable();
    assert_eq!(coords.len(), 25);
    assert_eq!(coords[0], (-2, -2));
    assert_eq!(coords[24], (2, 2));

    let first = &chunks[0];
    assert_eq!(first[9], ORDER_COLUMNS);
    let len = i32::from_be_bytes([first[10], first[11], first[12], first[13]]);
    assert_eq!(len as usize, first.len() - 14);

    // 队列清空后不再产生下行
    assert!(state.tick().await.is_empty());
}

// 视野跨出分片边界时 那几块也要带真实地形 不能发空区块
#[tokio::test]
async fn chunks_across_region_border_have_terrain() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

    let (chunks, _) = drain(&mut state).await;
    let border = chunks
        .iter()
        .find(|packet| chunk_coord(packet) == (-2, -2))
        .expect("缺少边界区块");
    // 包 14 字节头之后是方块数组 索引 4 即区块内 (0, 4, 0)
    assert_eq!(border[14 + 4], crate::level::generator::flat::GRASS);
}

// 客户端主动请求时只回半径更新 不重复下发区块
#[tokio::test]
async fn chunk_radius_request_only_updates_radius() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

    let commands = state.handle_game_packet(addr(), &request_radius(8));
    let packets = bodies(&commands[0]);
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0][0], CHUNK_RADIUS_UPDATE);
    assert_eq!(&packets[0][1..5], &DEFAULT_VIEW_DISTANCE.to_be_bytes());
}

// 走出初始 5x5 后只补新进入视野的区块 已发过的不重复发
#[tokio::test]
async fn moving_streams_new_chunks_only() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

    // 先吐完初始 25 个区块与出生状态
    let (initial, tail) = drain(&mut state).await;
    assert_eq!(initial.len(), 25);
    assert_eq!(
        tail,
        vec![
            PLAYER_LIST,
            TEXT,
            CONTAINER_SET_CONTENT,
            CONTAINER_SET_CONTENT,
            RESPAWN,
            PLAY_STATUS
        ]
    );

    // 走到区块 (1, 0)：视野从 x[-2..2] 变成 x[-1..3] 只新增 x = 3 那一列
    state.handle_game_packet(addr(), &move_player(16.5, 5.0, 0.5));
    let (added, tail) = drain(&mut state).await;
    let mut coords: Vec<(i32, i32)> = added.iter().map(chunk_coord).collect();
    coords.sort_unstable();
    assert_eq!(coords, vec![(3, -2), (3, -1), (3, 0), (3, 1), (3, 2)]);
    assert!(tail.is_empty());

    // 同一位置再上报不应重复下发
    state.handle_game_packet(addr(), &move_player(16.5, 5.0, 0.5));
    let extra = packet_ids(&state.tick().await);
    assert!(extra.is_empty(), "同一位置不该再下发：{extra:?}");
}

// tick 产生的下行按目标地址聚合包 id
async fn drain_by_peer(state: &mut NetworkState) -> HashMap<SocketAddr, Vec<u8>> {
    let mut out: HashMap<SocketAddr, Vec<u8>> = HashMap::new();
    for cmd in &pump(state).await {
        let SendCmd::GamePackets { addr, bodies } = cmd else {
            continue;
        };
        let entry: &mut Vec<u8> = out.entry(*addr).or_default();
        for body in bodies {
            if body[0] != SET_TIME {
                entry.push(body[0]);
            }
        }
    }
    out
}

fn add_player_name(packet: &Bytes) -> String {
    let len = u16::from_be_bytes([packet[17], packet[18]]) as usize;
    String::from_utf8(packet[19..19 + len].to_vec()).unwrap()
}

fn add_player_eid(packet: &Bytes) -> i64 {
    let start = 19 + u16::from_be_bytes([packet[17], packet[18]]) as usize;
    i64::from_be_bytes(packet[start..start + 8].try_into().unwrap())
}

// 后进玩家要看到先到的 先到的也要看到后进的
#[tokio::test]
async fn players_see_each_other() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = peer(50001);
    let bob = peer(50002);

    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    let first = drain_by_peer(&mut state).await;
    // 场上只有自己 只应有自己的列表项 没有实体 且列表项必须排在出生包之前
    let packets = &first[&alice];
    let list_pos = packets.iter().rposition(|p| *p == PLAYER_LIST).unwrap();
    let spawn_pos = packets.iter().rposition(|p| *p == PLAY_STATUS).unwrap();
    assert!(list_pos < spawn_pos, "玩家列表要早于 PLAYER_SPAWN");
    assert!(!packets.contains(&ADD_PLAYER));

    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));
    let second = drain_by_peer(&mut state).await;

    // bob 收到自己的列表项 + alice 的列表项与实体
    assert!(second[&bob].contains(&PLAYER_LIST));
    assert!(second[&bob].contains(&ADD_PLAYER));
    // alice 收到 bob 的列表项、实体与加入提示
    assert_eq!(second[&alice], vec![PLAYER_LIST, ADD_PLAYER, TEXT]);
}

// 加入与离开各给其他玩家发一条系统提示
#[tokio::test]
async fn join_and_leave_are_announced() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;

    let joined = state.on_player_spawned(bob);
    let bodies = bodies_of(&joined, alice);
    let notice = bodies
        .iter()
        .find(|body| body[0] == TEXT)
        .expect("加入时要通知其他玩家");
    let text = TextPacket::decode(notice).unwrap();
    assert_eq!(text.kind, TYPE_RAW);
    assert_eq!(text.message, "§ebob joined the game");

    let left = state.on_session_closed(0, &bob, "Disconnected");
    let bodies = bodies_of(&left, alice);
    let notice = bodies
        .iter()
        .find(|body| body[0] == TEXT)
        .expect("离开时要通知其他玩家");
    assert_eq!(
        TextPacket::decode(notice).unwrap().message,
        "§ebob left the game"
    );
}

// 实体 id 从 1 开始 0 留给客户端自己的实体
#[tokio::test]
async fn entity_ids_start_from_one() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = peer(50001);
    let bob = peer(50002);

    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));

    let mut seen = Vec::new();
    for cmd in &pump(&mut state).await {
        let SendCmd::GamePackets { addr, bodies } = cmd else {
            continue;
        };
        if *addr != bob {
            continue;
        }
        for body in bodies {
            if body[0] == ADD_PLAYER {
                seen.push((add_player_name(body), add_player_eid(body)));
            }
        }
    }
    assert_eq!(seen, vec![("alice".to_string(), 1)]);
}

// 移动只广播给其他玩家 自己不需要 MoveEntity
#[tokio::test]
async fn movement_is_broadcast_to_others() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = peer(50001);
    let bob = peer(50002);

    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));
    drain_by_peer(&mut state).await;

    // 移动包在分片里消费 广播随回执直接返回 分片后续产出再补上
    let mut commands = state.handle_game_packet(alice, &move_player(2.5, 5.0, 3.5));
    commands.extend(pump(&mut state).await);
    assert_eq!(packet_ids_of(&commands, bob), vec![MOVE_ENTITY]);
    assert!(packet_ids_of(&commands, alice).is_empty());

    // 包里的 y 是眼睛位置 转发给其他玩家时也要用眼睛位置 否则实体陷进地里
    let bob_bodies = bodies_of(&commands, bob);
    let packet = &bob_bodies[0];
    // pid(1) | count(4) | eid(8) | x y z
    let x = f32::from_be_bytes(packet[13..17].try_into().unwrap());
    let y = f32::from_be_bytes(packet[17..21].try_into().unwrap());
    assert!((x - 2.5).abs() < 1e-3);
    assert!((y - 5.0).abs() < 1e-3);
}

// 断线要通知其他玩家移除实体与列表项
#[tokio::test]
async fn disconnect_notifies_others() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = peer(50001);
    let bob = peer(50002);

    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));
    drain_by_peer(&mut state).await;

    let commands = state.on_session_closed(0, &bob, "Disconnected");
    assert_eq!(commands.len(), 1);
    let packets = bodies(&commands[0]);
    assert_eq!(packets[0][0], REMOVE_PLAYER);
    assert_eq!(packets[1][0], PLAYER_LIST);
    // bob 的实体 id 是 2
    assert_eq!(&packets[0][1..9], &2i64.to_be_bytes());
    assert_eq!(state.player_count(), 1);
}

// 区块还没发完的玩家不出现在别人的世界里
#[tokio::test]
async fn players_waiting_for_chunks_are_invisible() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = peer(50001);
    let bob = peer(50002);

    state.handle_game_packet(alice, &login_packet_for("alice", [0x11; 16]));
    drain_by_peer(&mut state).await;

    // bob 刚登录 区块还在 region 线程里排队 这时他还没出生
    state.handle_game_packet(bob, &login_packet_for("bob", [0x22; 16]));
    assert!(!state.players[&bob].spawned);
    // 已出生的 alice 移动 不该同步给还没出生的 bob
    let commands = state.handle_game_packet(alice, &move_player(2.5, 5.0, 3.5));
    assert!(packet_ids_of(&commands, bob).is_empty());

    // 出生之后 alice 才会收到 bob 的实体
    let rest = drain_by_peer(&mut state).await;
    assert!(rest[&alice].contains(&ADD_PLAYER));
}

fn player_action_packet(eid: i64, action: i32) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(PLAYER_ACTION);
    w.put_i64(eid);
    w.put_i32(action);
    for _ in 0..4 {
        w.put_i32(0);
    }
    w.freeze().to_vec()
}

fn swing_packet(eid: i64) -> Vec<u8> {
    let mut w = BinaryWriter::new();
    w.put_u8(GAME_PACKET_PREFIX);
    w.put_u8(ANIMATE);
    w.put_u8(ACTION_SWING_ARM);
    w.put_i64(eid);
    w.freeze().to_vec()
}

// 控制台命令不带斜杠也能执行 结果与发送者解耦
#[tokio::test]
async fn console_style_command_without_slash() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let output = state.run_command("time set night");
    assert_eq!(output.messages, vec!["Set the time to 14000".to_string()]);
    assert_eq!(state.world_time.time(), 14000);
}

// 斜杠命令在 global 层消费 不会走进聊天广播
#[tokio::test]
async fn slash_command_is_not_broadcast_as_chat() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());
    state.ops.set("Xiao_QDev", true); // /time 是 OP 专属

    let commands = state.handle_game_packet(addr(), &chat_packet("/time query"));
    let bodies = bodies_of(&commands, addr());
    assert_eq!(bodies.len(), 1, "命令只回给发起者一条");
    let text = TextPacket::decode(&bodies[0][..]).unwrap();
    assert_eq!(text.kind, TYPE_RAW);
    assert!(text.message.starts_with("Time is "), "{}", text.message);
}

// /say 控制台发起时以 Server 身份广播给全体在线玩家
#[tokio::test]
async fn console_say_broadcasts_to_every_player() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let output = state.run_command("say hello world");
    assert_eq!(output.messages, vec!["[Server] hello world".to_string()]);
    for player in [alice, bob] {
        let texts: Vec<_> = bodies_of(&output.sync, player)
            .into_iter()
            .filter(|body| body[0] == TEXT)
            .collect();
        assert_eq!(texts.len(), 1, "每个在线玩家各收一条广播");
        assert_eq!(
            TextPacket::decode(&texts[0][..]).unwrap().message,
            "[Server] hello world"
        );
    }
}

// 玩家 /say 用服务端记录的用户名 客户端自带的 source 不可信
#[tokio::test]
async fn player_say_uses_username_without_duplicate_reply() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("/say hi there"));
    for player in [alice, bob] {
        let texts: Vec<_> = bodies_of(&commands, player)
            .into_iter()
            .filter(|body| body[0] == TEXT)
            .collect();
        assert_eq!(texts.len(), 1, "广播只发一条 不能再补一条 messages 回复");
        assert_eq!(
            TextPacket::decode(&texts[0][..]).unwrap().message,
            "[alice] hi there"
        );
    }

    let commands = state.handle_game_packet(alice, &chat_packet("/say"));
    let texts: Vec<_> = bodies_of(&commands, alice)
        .into_iter()
        .filter(|body| body[0] == TEXT)
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(
        TextPacket::decode(&texts[0][..]).unwrap().message,
        command::say::SAY_USAGE
    );
}

// 玩家 /tell 只把私聊定向发给目标 发起者收到回执 全服其他人看不到
#[tokio::test]
async fn tell_sends_private_message_to_target_only() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("/tell bob hi"));
    // bob 收到私聊 内容不是公聊格式
    let bob_texts: Vec<_> = bodies_of(&commands, bob)
        .into_iter()
        .filter(|body| body[0] == TEXT)
        .collect();
    assert_eq!(bob_texts.len(), 1);
    assert_eq!(
        TextPacket::decode(&bob_texts[0][..]).unwrap().message,
        "alice whispers to you: hi"
    );
    // alice 收到回执
    let alice_texts: Vec<_> = bodies_of(&commands, alice)
        .into_iter()
        .filter(|body| body[0] == TEXT)
        .collect();
    assert_eq!(alice_texts.len(), 1);
    assert_eq!(
        TextPacket::decode(&alice_texts[0][..]).unwrap().message,
        "You whisper to bob: hi"
    );
}

// 目标不在线 /tell 只回一句提示给发起者 不广播
#[tokio::test]
async fn tell_to_offline_player_replies_not_online() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("/tell ghost hi"));
    let alice_texts: Vec<_> = bodies_of(&commands, alice)
        .into_iter()
        .filter(|body| body[0] == TEXT)
        .collect();
    assert_eq!(alice_texts.len(), 1);
    assert!(
        TextPacket::decode(&alice_texts[0][..])
            .unwrap()
            .message
            .contains("not online")
    );
    // 其他在线玩家(以及不存在的 ghost)都不应收到任何包
    assert!(bodies_of(&commands, bob).is_empty());
}

// 玩家聊天经 on_text 广播给其他玩家 同时在服务端控制台镜像一份 "<name> msg"
#[tokio::test]
async fn player_chat_is_broadcast_and_logged_to_server() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let commands = state.handle_game_packet(alice, &chat_packet("hello world"));
    // bob 收到聊天的 TEXT 包
    let bob_texts: Vec<_> = bodies_of(&commands, bob)
        .into_iter()
        .filter(|body| body[0] == TEXT)
        .collect();
    assert_eq!(bob_texts.len(), 1);
    assert_eq!(
        TextPacket::decode(&bob_texts[0][..]).unwrap().message,
        "hello world"
    );
    // 服务端控制台镜像 "<alice> hello world"
    let logs: Vec<String> = commands
        .iter()
        .filter_map(|cmd| match cmd {
            SendCmd::Console(line) => Some(line.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(logs, vec!["<alice> hello world".to_string()]);
}

// /time set 改世界时间并立刻向全体同步 SetTime
#[tokio::test]
async fn time_set_changes_world_time_and_syncs() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());
    state.ops.set("Xiao_QDev", true); // /time 是 OP 专属

    let commands = state.handle_game_packet(addr(), &chat_packet("/time set day"));
    assert_eq!(state.world_time.time(), 0);
    assert!(
        bodies_of(&commands, addr())
            .iter()
            .any(|body| body[0] == SET_TIME),
        "时间变更后要同步 SetTime"
    );

    let commands = state.handle_game_packet(addr(), &chat_packet("/time query"));
    let bodies = bodies_of(&commands, addr());
    assert_eq!(
        TextPacket::decode(&bodies[0][..]).unwrap().message,
        "Time is 0"
    );
}

// 世界时间每 tick +1 每 200 tick 向全体同步一次
#[tokio::test]
async fn world_time_advances_and_syncs_every_200_ticks() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());
    for _ in 0..199 {
        state.tick().await;
    }
    assert_eq!(state.world_time.time(), 6000 + 199);
    let commands = state.tick().await;
    assert!(
        bodies_of(&commands, addr())
            .iter()
            .any(|body| body[0] == SET_TIME),
        "第 200 tick 要发 SetTime"
    );
    assert_eq!(state.world_time.time(), 6000 + 200);
}

// 下蹲与疾跑通过实体数据包同步给其他玩家
#[tokio::test]
async fn sneaking_and_sprinting_are_broadcast() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let eid = state.players[&alice].eid;

    let commands = state.handle_game_packet(alice, &player_action_packet(eid, ACTION_START_SNEAK));
    assert_eq!(packet_ids_of(&commands, bob), vec![SET_ENTITY_DATA]);
    assert!(packet_ids_of(&commands, alice).is_empty());
    let body = bodies_of(&commands, bob)[0].clone();
    assert_eq!(&body[1..9], &eid.to_be_bytes());
    assert_eq!(body[10], FLAG_SNEAKING);

    // 疾跑叠加后两位都要有
    let commands = state.handle_game_packet(alice, &player_action_packet(eid, ACTION_START_SPRINT));
    assert_eq!(
        bodies_of(&commands, bob)[0][10],
        FLAG_SNEAKING | FLAG_SPRINTING
    );

    // 重复的启动动作不重复广播
    assert!(
        state
            .handle_game_packet(alice, &player_action_packet(eid, ACTION_START_SPRINT))
            .is_empty()
    );

    // 停止下蹲 只留疾跑
    let commands = state.handle_game_packet(alice, &player_action_packet(eid, ACTION_STOP_SNEAK));
    assert_eq!(bodies_of(&commands, bob)[0][10], FLAG_SPRINTING);

    // 停止疾跑 标志清零
    let commands = state.handle_game_packet(alice, &player_action_packet(eid, ACTION_STOP_SPRINT));
    assert_eq!(bodies_of(&commands, bob)[0][10], 0);
}

// 客户端上报的 eid 不可信 广播时一律换成服务端分配的
#[tokio::test]
async fn action_eid_is_rewritten_to_server_side() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    // 客户端自视角发 0 与 Nukkit 一致 服务端不校验
    let commands = state.handle_game_packet(alice, &player_action_packet(0, ACTION_START_SNEAK));
    let eid = state.players[&alice].eid;
    assert_eq!(&bodies_of(&commands, bob)[0][1..9], &eid.to_be_bytes());
}

// 挥手动画转发给其他玩家
#[tokio::test]
async fn swing_is_forwarded_to_others() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    let eid = state.players[&alice].eid;

    let commands = state.handle_game_packet(alice, &swing_packet(eid));
    assert_eq!(packet_ids_of(&commands, bob), vec![ANIMATE]);
    assert!(packet_ids_of(&commands, alice).is_empty());
    assert_eq!(bodies_of(&commands, bob)[0][1], ACTION_SWING_ARM);

    // eid 一律用服务端分配的值转发
    let commands = state.handle_game_packet(alice, &swing_packet(0));
    assert_eq!(&bodies_of(&commands, bob)[0][2..10], &eid.to_be_bytes());
}

// 噪声地形下出生点要落在陆地上 而且脚踩实体方块
#[tokio::test]
async fn spawn_point_lands_on_land() {
    let path = std::env::temp_dir().join(format!(
        "violin-spawn-{}-{}.redb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut state = NetworkState::with_loader(
        GAMEMODE_SURVIVAL,
        ChunkLoader::open(&path).unwrap(),
        Generator::terrain(1337),
        1337,
        DEFAULT_VIEW_DISTANCE,
        1,
        "world".to_string(),
    );

    let (block, position) = state.spawn_point();
    assert!(block.1 > SEA_LEVEL + 1, "出生点在水下: {block:?}");
    assert_eq!(state.block_at(block.0, block.1, block.2), 0);
    assert_ne!(state.block_at(block.0, block.1 - 1, block.2), 0);
    assert_eq!(
        position,
        (block.0 as f32 + 0.5, block.1 as f32, block.2 as f32 + 0.5)
    );

    let _ = std::fs::remove_file(&path);
}

// RTG 地形下出生点也要脚踩实体上方是空气
#[tokio::test]
async fn spawn_point_lands_on_land_rtg() {
    let path = std::env::temp_dir().join(format!(
        "violin-spawn-rtg-{}-{}.redb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut state = NetworkState::with_loader(
        GAMEMODE_SURVIVAL,
        ChunkLoader::open(&path).unwrap(),
        Generator::realistic(1337),
        1337,
        DEFAULT_VIEW_DISTANCE,
        1,
        "world".to_string(),
    );
    let (block, _) = state.spawn_point();
    let (x, y, z) = block;
    assert_eq!(state.block_at(x, y, z), 0, "出生格不是空气: {block:?}");
    assert_ne!(state.block_at(x, y - 1, z), 0, "脚下不是实体: {block:?}");
    assert_eq!(state.block_at(x, y + 1, z), 0, "头顶不是空气: {block:?}");

    // 下发给客户端的实体 y 要带眼睛高度 否则玩家会被埋进地里约两格
    use crate::network::protocol::move_player::EYE_HEIGHT;
    use crate::network::protocol::login::LoginPacket;
    let login = LoginPacket::decode(&login_packet()[1..]).unwrap();
    let packets = crate::network::login::login_sequence(
        &login,
        GAMEMODE_SURVIVAL,
        2,
        1337,
        block,
        (x as f32 + 0.5, y as f32, z as f32 + 0.5),
        &crate::level::WorldTime::default(),
    );
    let start_game = &packets[1];
    assert_eq!(start_game[0], START_GAME);
    let sent_y = f32::from_be_bytes(start_game[38..42].try_into().unwrap());
    assert!(
        (sent_y - (y as f32 + EYE_HEIGHT)).abs() < 0.01,
        "StartGame 的 y 没带眼睛高度: {sent_y}"
    );
    let _ = std::fs::remove_file(&path);
}

// 出生点选点耗时 33x33 列现在一次批量取回 不再逐列跨线程
#[tokio::test]
#[ignore]
async fn probe_spawn_point_timing() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    // 第一次会把周边区块生成出来 预热一下再计时
    let _ = state.spawn_point();

    let t = std::time::Instant::now();
    let (block, _) = state.spawn_point();
    println!("spawn_point: {:?} -> {block:?}", t.elapsed());
}

// 创造模式进入游戏时 服务端下发的创造背包(window 0x79) 让客户端渲染创造物品栏
#[test]
fn creative_spawn_sends_creative_inventory() {
    use crate::network::protocol::container_set_content::SPECIAL_CREATIVE;
    let pkts = spawn_sequence((0.0, 4.0, 0.0), GAMEMODE_CREATIVE);
    assert!(
        pkts.iter()
            .any(|b| b[0] == CONTAINER_SET_CONTENT && b[1] == SPECIAL_CREATIVE)
    );
}

// 出生时的 Respawn 包 y 也要带眼睛高度 否则 StartGame 抬起后又被拉回地下
#[test]
fn spawn_respawn_carries_eye_height() {
    use crate::network::protocol::move_player::EYE_HEIGHT;
    let pkts = spawn_sequence((0.0, 4.0, 0.0), GAMEMODE_SURVIVAL);
    let respawn = pkts.iter().find(|b| b[0] == RESPAWN).unwrap();
    let sent_y = f32::from_be_bytes(respawn[5..9].try_into().unwrap());
    assert!(
        (sent_y - (4.0 + EYE_HEIGHT)).abs() < 0.01,
        "Respawn 的 y 没带眼睛高度: {sent_y}"
    );
}

// 生存模式不发创造背包 只发普通背包与盔甲
#[test]
fn survival_spawn_omits_creative_inventory() {
    use crate::network::protocol::container_set_content::SPECIAL_CREATIVE;
    let pkts = spawn_sequence((0.0, 4.0, 0.0), 0);
    assert!(
        !pkts.iter()
            .any(|b| b[0] == CONTAINER_SET_CONTENT && b[1] == SPECIAL_CREATIVE)
    );
}

// 玩家下线按用户名存档 重新登录应恢复位置而不是回到出生点
#[tokio::test]
async fn player_data_persists_across_sessions() {
    use crate::entity::PlayerData;

    let dir = std::env::temp_dir().join(format!(
        "violin-player-save-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    // 第一段：登录 -> 走到指定位置 -> 断线存档
    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&dir).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        let alice = peer(50004);
        state.handle_game_packet(alice, &login_packet_for("alice", [0x44; 16]));
        drain_by_peer(&mut state).await;
        // 进入即存 登录后应出现 players/alice.dat
        wait_player_data(&dir, login_uuid(0x44), "alice").await;
        // move_player 传的是眼睛坐标 存档里记脚下坐标
        state.handle_game_packet(alice, &move_player(12.5, 30.0 + 1.62, -4.5));
        drain_by_peer(&mut state).await;
        state.on_session_closed(0, &alice, "disconnect");
    }

    let data = PlayerData::load(&dir, login_uuid(0x44), "alice").expect("下线应写出玩家存档");
    assert!((data.position.0 - 12.5).abs() < 0.01);
    assert!((data.position.1 - 30.0).abs() < 0.01);
    assert!((data.position.2 + 4.5).abs() < 0.01);

    // 第二段：重新登录 位置应从存档恢复 再断线存档仍保持一致
    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&dir).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
            "world".to_string(),
        );
        let alice = peer(50005);
        state.handle_game_packet(alice, &login_packet_for("alice", [0x44; 16]));
        drain_by_peer(&mut state).await;
        state.on_session_closed(0, &alice, "disconnect");
    }
    let back = PlayerData::load(&dir, login_uuid(0x44), "alice").unwrap();
    assert!((back.position.0 - 12.5).abs() < 0.01);
    assert!((back.position.1 - 30.0).abs() < 0.01);
    assert!((back.position.2 + 4.5).abs() < 0.01);

    let _ = std::fs::remove_dir_all(&dir);
}

// 停机兜底 没有 SessionClosed(Ctrl+C 关服) 也要把在线玩家写盘
#[tokio::test]
async fn shutdown_saves_online_players() {
    use crate::entity::PlayerData;

    let dir = std::env::temp_dir().join(format!(
        "violin-shutdown-save-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut state = NetworkState::with_loader(
        GAMEMODE_SURVIVAL,
        ChunkLoader::open(&dir).unwrap(),
        Generator::flat(),
        0,
        DEFAULT_VIEW_DISTANCE,
        1,
        "world".to_string(),
    );
    let alice = peer(50006);
    state.handle_game_packet(alice, &login_packet_for("alice", [0x55; 16]));
    drain_by_peer(&mut state).await;

    // 模拟直接 Ctrl+C 关服 不经过 on_session_closed
    state.save_all_players();

    let data = PlayerData::load(&dir, login_uuid(0x55), "alice").expect("停机兜底应写出在线玩家存档");
    assert!(data.position.1 > 0.0);
    let _ = std::fs::remove_dir_all(&dir);
}

// 同端点立刻重连 新会话顶替旧会话后 旧会话迟到的关闭不能踢掉新玩家
#[tokio::test]
async fn stale_session_close_does_not_kick_reconnected_player() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = addr();

    state.on_session_opened(1, &alice);
    state.handle_game_packet(alice, &login_packet());
    assert!(state.players.contains_key(&alice));

    // 客户端重连 同一端点换新会话
    state.on_session_opened(2, &alice);
    // 旧会话退役的关闭事件迟到 必须忽略
    let out = state.on_session_closed(1, &alice, "replaced");
    assert!(out.is_empty());
    assert!(
        state.players.contains_key(&alice),
        "旧会话收尾不能清掉新会话的玩家"
    );

    // 当前会话自己断开才清理
    let _ = state.on_session_closed(2, &alice, "Disconnected");
    assert!(!state.players.contains_key(&alice));
}

// 旧会话残留的游戏包按 id 丢弃 不能打进新会话
#[test]
fn stale_session_packets_are_filtered_by_id() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = addr();
    state.on_session_opened(7, &alice);
    assert!(state.session_matches(7, &alice));
    assert!(!state.session_matches(6, &alice));
}

// 换端点立刻重连 旧端点超时收尾不能覆盖新会话的存档
#[tokio::test]
async fn stale_session_does_not_overwrite_reconnected_save() {
    use crate::entity::PlayerData;

    let dir = std::env::temp_dir().join(format!(
        "violin-reconnect-save-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut state = NetworkState::with_loader(
        GAMEMODE_SURVIVAL,
        ChunkLoader::open(&dir).unwrap(),
        Generator::flat(),
        0,
        DEFAULT_VIEW_DISTANCE,
        1,
        "world".to_string(),
    );

    // 旧连接 id=1 走到远处
    let old = peer(50007);
    state.on_session_opened(1, &old);
    state.handle_game_packet(old, &login_packet_for("alice", [0x77; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(old, &move_player(40.5, 30.0 + 1.62, 40.5));
    drain_by_peer(&mut state).await;

    // 客户端换端点重连 id=2 登录时进入即存
    let new = peer(50008);
    state.on_session_opened(2, &new);
    state.handle_game_packet(new, &login_packet_for("alice", [0x77; 16]));
    drain_by_peer(&mut state).await;

    // 旧端点 10 秒后超时收尾 同名 alice 已在线 不能再写盘
    state.on_session_closed(1, &old, "timeout");

    let data = PlayerData::load(&dir, login_uuid(0x77), "alice").expect("新会话存档应保留");
    assert!(
        (data.position.0 - 40.5).abs() > 0.01,
        "旧会话的迟到存档不能覆盖新会话"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// 同名的另一台设备带来的是另一个 uuid 只能拿到自己那份存档
#[tokio::test]
async fn same_name_different_uuid_gets_own_save() {
    let dir = std::env::temp_dir().join(format!(
        "violin-uuid-save-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut state = NetworkState::with_loader(
        GAMEMODE_SURVIVAL,
        ChunkLoader::open(&dir).unwrap(),
        Generator::flat(),
        0,
        DEFAULT_VIEW_DISTANCE,
        1,
        "world".to_string(),
    );

    // 设备 A 用 alice 这个名字走到远处并下线
    let device_a = peer(50009);
    state.handle_game_packet(device_a, &login_packet_for("alice", [0xa1; 16]));
    drain_by_peer(&mut state).await;
    state.handle_game_packet(device_a, &move_player(40.5, 30.0 + 1.62, 40.5));
    drain_by_peer(&mut state).await;
    state.on_session_closed(0, &device_a, "disconnect");

    // 设备 B 也叫 alice 但 uuid 不同 读不到 A 的存档
    let device_b = peer(50010);
    state.handle_game_packet(device_b, &login_packet_for("alice", [0xb2; 16]));
    drain_by_peer(&mut state).await;

    let mine = wait_player_data(&dir, login_uuid(0xb2), "alice").await;
    assert!(
        (mine.position.0 - 40.5).abs() > 0.01,
        "同名换 uuid 不能读到别人的坐标"
    );
    let theirs = wait_player_data(&dir, login_uuid(0xa1), "alice").await;
    assert!((theirs.position.0 - 40.5).abs() < 0.01);

    let _ = std::fs::remove_dir_all(&dir);
}

// 传送落地时客户端要收到 MODE_RESET 落点
// 服务端内部坐标是脚部 包里要换成客户端用的眼睛高度
#[tokio::test]
async fn tp_resets_client_at_destination() {
    use crate::network::protocol::move_player::{EYE_HEIGHT, MODE_RESET};

    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = addr();
    state.handle_game_packet(alice, &login_packet());
    drain_by_peer(&mut state).await;

    let out = state.handle_game_packet(alice, &chat_packet("/tp 100 20 100"));
    // 回执照 Nukkit 的 commands.tp.success.coordinates 打给发起者
    assert!(packet_ids_of(&out, alice).contains(&TEXT));
    // 镜像立刻跟上 权威位置由属主分片在 tick 里落地
    assert_eq!(state.players[&alice].position, (100.0, 20.0, 100.0));

    let commands = pump_until(&mut state, |ids| ids.contains(&MOVE_PLAYER)).await;
    let bodies = bodies_of(&commands, alice);
    let reset = bodies
        .iter()
        .find(|body| body[0] == MOVE_PLAYER)
        .expect("传送后应下发落点");
    // 0 pid | 1..9 eid | 9..13 x | 13..17 y | 17..21 z | ... | 33 mode
    assert_eq!(reset[33], MODE_RESET);
    let y = f32::from_be_bytes(reset[13..17].try_into().unwrap());
    assert!(
        (y - (20.0 + EYE_HEIGHT)).abs() < 0.001,
        "reset 包的 y 没带眼睛高度: {y}"
    );
    assert_eq!(state.players[&alice].position, (100.0, 20.0, 100.0));
}

// 跨 512 分片边界传送 归属表要跟着换 之后包还要能正常处理
#[tokio::test]
async fn tp_across_region_updates_ownership() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = addr();
    state.handle_game_packet(alice, &login_packet());
    drain_by_peer(&mut state).await;

    state.handle_game_packet(alice, &chat_packet("/tp 600 20 5"));
    pump_until(&mut state, |ids| ids.contains(&MOVE_PLAYER)).await;

    assert_eq!(
        state.player_index[&alice],
        crate::level::region::RegionId { x: 1, z: 0 }
    );
    assert_eq!(state.players[&alice].position, (600.0, 20.0, 5.0));

    // 客户端确认落点 纠正标记解除 随后的正常移动被接受
    state.handle_game_packet(alice, &move_player(600.0, 20.0 + 1.62, 5.0));
    state.handle_game_packet(alice, &move_player(600.5, 20.0 + 1.62, 5.5));
    assert_eq!(state.players[&alice].position, (600.5, 20.0, 5.5));
}

// 传送前发出的旧移动包不能把玩家拉回原处 照搬 Nukkit 的 forceMovement 纠正
#[tokio::test]
async fn stale_move_after_tp_does_not_drag_player_back() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let alice = addr();
    state.handle_game_packet(alice, &login_packet());
    drain_by_peer(&mut state).await;

    state.handle_game_packet(alice, &chat_packet("/tp 100 20 100"));
    pump_until(&mut state, |ids| ids.contains(&MOVE_PLAYER)).await;

    // 还在路上的旧坐标到了 分片要再补一次落点并且不接受
    let out = state.handle_game_packet(alice, &move_player(8.5, 5.0 + 1.62, 8.5));
    assert!(
        packet_ids_of(&out, alice).contains(&MOVE_PLAYER),
        "旧坐标应触发一次落点纠正"
    );
    assert_eq!(state.players[&alice].position, (100.0, 20.0, 100.0));

    // 客户端真正到位的确认包被接受 玩家可以正常移动
    state.handle_game_packet(alice, &move_player(100.0, 20.0 + 1.62, 100.0));
    state.handle_game_packet(alice, &move_player(100.5, 20.0 + 1.62, 100.5));
    assert_eq!(state.players[&alice].position, (100.5, 20.0, 100.5));
}

// /tps 只读主循环写进来的采样 掉拍时 TPS 要跟着掉 而不是永远报 20
#[tokio::test]
async fn tps_command_reads_tick_meter() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    // 一拍都还没跑完时不能报 0 会给标称值加提示
    let out = state.run_command("/tps");
    assert_eq!(out.messages[0], "§bServer Health Report");
    assert!(out.messages[3].contains("no sample yet"), "{}", out.messages[3]);

    // 20 拍满速 每拍只花 1ms
    for _ in 0..20 {
        state.record_tick(Duration::from_millis(50), Duration::from_millis(1));
    }
    let out = state.run_command("tps");
    assert!(
        out.messages[3].contains("§a20.00 TPS") && out.messages[3].contains("1.00/1.00 ms"),
        "主循环行不对: {}",
        out.messages[3]
    );

    // 换一台"卡住"的服务器 100ms 一拍即 10 TPS 要标红
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    for _ in 0..20 {
        state.record_tick(Duration::from_millis(100), Duration::from_millis(90));
    }
    let out = state.run_command("tps");
    assert!(out.messages[3].contains("§c10.00 TPS"), "{}", out.messages[3]);
    // 每拍落后 50ms 20 拍累计 1000ms
    assert!(
        out.messages[3].contains("lag §a1000 ms"),
        "lag 统计不对: {}",
        out.messages[3]
    );
}

// /gamemode 切换全服模式: 更新状态 + 给全体下发 SetPlayerGameType 与飞行权限
#[tokio::test]
async fn gamemode_command_updates_state_and_broadcasts() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    state.ops.set("alice", true); // /gamemode 是 OP 专属
    let out = state.run_command_from("/gamemode creative", Some(alice));
    // 服务端全局状态已切到创造
    assert_eq!(state.gamemode, GAMEMODE_CREATIVE);
    assert_eq!(out.messages, vec!["Game mode is now Creative".to_string()]);
    // 两个在线玩家都收到模式包 + 飞行权限包
    for peer in [alice, bob] {
        let ids = packet_ids_of(&out.sync, peer);
        assert!(ids.contains(&SET_PLAYER_GAMETYPE), "模式包未发给 {peer}");
        assert!(ids.contains(&ADVENTURE_SETTINGS), "飞行权限包未发给 {peer}");
    }
    // 切回生存
    let out = state.run_command_from("/gamemode 0", Some(alice));
    assert_eq!(state.gamemode, GAMEMODE_SURVIVAL);
    assert_eq!(out.messages, vec!["Game mode is now Survival".to_string()]);
    // 用法错误不改变状态也不广播
    let out = state.run_command_from("/gamemode 3", Some(alice));
    assert_eq!(state.gamemode, GAMEMODE_SURVIVAL);
    assert!(out.sync.is_empty());
    assert_eq!(out.messages, vec!["Usage: /gamemode <survival|creative>".to_string()]);
}

// /difficulty 切换全服难度: 更新状态 + 广播 SetDifficulty + 新玩家登录读当前难度
#[tokio::test]
async fn difficulty_command_updates_state_and_broadcasts() {
    let (mut state, alice, bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    state.ops.set("alice", true); // /difficulty 是 OP 专属
    let out = state.run_command_from("/difficulty hard", Some(alice));
    assert_eq!(state.difficulty, 3);
    assert_eq!(out.messages, vec!["Difficulty is now Hard".to_string()]);
    for peer in [alice, bob] {
        assert!(
            packet_ids_of(&out.sync, peer).contains(&SET_DIFFICULTY),
            "难度包未发给 {peer}"
        );
    }
    // 用法错误不改变状态也不广播
    let out = state.run_command_from("/difficulty nope", Some(alice));
    assert_eq!(state.difficulty, 3);
    assert!(out.sync.is_empty());
}

// 权限: 普通玩家用 OP 专属命令被拒 且不产生任何状态改动或下行包
#[tokio::test]
async fn non_op_player_is_denied_op_only_commands() {
    let (mut state, alice, _bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    // alice 不在名单里
    let out = state.run_command_from("/gamemode creative", Some(alice));
    assert_eq!(out.messages, vec!["§cYou are not allowed to use this command".to_string()]);
    assert!(out.sync.is_empty());
    assert_eq!(state.gamemode, GAMEMODE_SURVIVAL, "被拒命令不能改全局状态");
    // 多个 OP 专属命令都拦
    for cmd in ["/difficulty hard", "/time set night", "/tp alice bob", "/stop"] {
        let out = state.run_command_from(cmd, Some(alice));
        assert_eq!(
            out.messages,
            vec!["§cYou are not allowed to use this command".to_string()],
            "{cmd} 应当被拒"
        );
    }
    assert!(!state.shutdown_requested, "/stop 被拒后不能停机");
    // 玩家可用的命令不受影响
    let out = state.run_command_from("/say hi", Some(alice));
    assert!(!out.messages.iter().any(|m| m.contains("not allowed")));
}

// 权限: 控制台不受名单限制 空名单也能用 op 加人 加完立即可用
#[tokio::test]
async fn console_is_always_op_and_manages_roster() {
    let (mut state, _alice, _bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    assert!(state.ops.is_empty());
    // 控制台 /op 无需自己是名单成员
    let out = state.run_command("op alice");
    assert_eq!(out.messages, vec!["Made alice an operator".to_string()]);
    assert!(state.ops.is_op("alice"));
    assert!(state.ops.is_op("ALICE"), "用户名大小写不敏感");
    // 重复添加给提示
    let out = state.run_command("op Alice");
    assert_eq!(out.messages, vec!["Alice is already an operator".to_string()]);
    // /ops 列出名单
    let out = state.run_command("ops");
    assert!(out.messages.iter().any(|m| m.contains("There are 1 ops")));
    assert!(out.messages.iter().any(|m| m.contains("alice")));
    // /deop 移除 之后该玩家立刻失去权限
    let out = state.run_command("deop alice");
    assert_eq!(out.messages, vec!["Took alice out of the operator list".to_string()]);
    assert!(!state.ops.is_op("alice"));
    let alice_addr = peer(50001);
    let out = state.run_command_from("/gamemode creative", Some(alice_addr));
    assert_eq!(out.messages, vec!["§cYou are not allowed to use this command".to_string()]);
}

// 权限: 名单内玩家可执行 OP 命令
#[tokio::test]
async fn op_player_can_run_op_commands() {
    let (mut state, alice, _bob) = spawn_two(GAMEMODE_SURVIVAL).await;
    state.ops.set("alice", true);
    let out = state.run_command_from("/gamemode creative", Some(alice));
    assert_eq!(out.messages, vec!["Game mode is now Creative".to_string()]);
    assert_eq!(state.gamemode, GAMEMODE_CREATIVE);
    // OP 玩家也能管理名单
    let out = state.run_command_from("/ops", Some(alice));
    assert!(out.messages.iter().any(|m| m.contains("alice")));
}

// 权限: /op /deop 参数不全给用法
#[tokio::test]
async fn op_commands_need_a_player_name() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let out = state.run_command("op");
    assert_eq!(out.messages, vec!["Usage: /op <player>".to_string()]);
    let out = state.run_command("deop");
    assert_eq!(out.messages, vec!["Usage: /deop <player>".to_string()]);
}
