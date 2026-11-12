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
    UPDATE_ATTRIBUTES, UPDATE_BLOCK,
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

fn addr() -> SocketAddr {
    "127.0.0.1:50000".parse().unwrap()
}

fn peer(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
}

fn login_packet() -> Vec<u8> {
    login_packet_for("Xiao_QDev", [0; 16])
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

fn packet_ids(commands: &[SendCmd]) -> Vec<u8> {
    commands
        .iter()
        .flat_map(|cmd| bodies(cmd).iter().map(|body| body[0]).collect::<Vec<_>>())
        .collect()
}

fn packet_ids_of(commands: &[SendCmd], target: SocketAddr) -> Vec<u8> {
    bodies_of(commands, target)
        .iter()
        .map(|body| body[0])
        .collect()
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

// 挖方块要改服务端世界 并在原位生成掉落物
#[tokio::test]
async fn digging_updates_world_and_spawns_drop() {
    let (mut state, alice, _) = spawn_two(GAMEMODE_SURVIVAL).await;
    assert_eq!(state.block_at(0, 4, 0), crate::level::generator::flat::GRASS);

    // 挖掘交给属主分片去做 结果以事件回来
    state.handle_game_packet(alice, &remove_block(0, 4, 0));
    let commands = pump(&mut state).await;
    assert_eq!(state.block_at(0, 4, 0), 0);
    // 两个已出生玩家各收方块更新与掉落物 背包不再立刻刷新
    assert_eq!(
        packet_ids(&commands),
        vec![UPDATE_BLOCK, UPDATE_BLOCK, ADD_ITEM_ENTITY, ADD_ITEM_ENTITY]
    );
    assert_eq!(state.drops.len(), 1);
    assert!(state.players[&alice].inventory[0].is_none());
}

// 改过的方块落盘后重新打开还在 没改过的部分仍是平坦地形
#[tokio::test]
async fn edited_block_survives_reload() {
    let path = std::env::temp_dir().join(format!(
        "mojor-net-persist-{}-{}.redb",
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
        );
        let alice = peer(50003);
        state.handle_game_packet(alice, &login_packet_for("alice", [0x33; 16]));
        drain_by_peer(&mut state).await;

        state.handle_game_packet(alice, &remove_block(0, 4, 0));
        // 挖掘在分片线程里完成 等结果回来再落盘
        drain_by_peer(&mut state).await;
        assert_eq!(state.block_at(0, 4, 0), 0);
        assert_eq!(state.flush_world().unwrap(), 1);
    }

    {
        let mut state = NetworkState::with_loader(
            GAMEMODE_SURVIVAL,
            ChunkLoader::open(&path).unwrap(),
            Generator::flat(),
            0,
            DEFAULT_VIEW_DISTANCE,
            1,
        );
        assert_eq!(state.block_at(0, 4, 0), 0);
        assert_eq!(state.block_at(0, 3, 0), crate::level::generator::flat::DIRT);
        assert_eq!(state.flush_world().unwrap(), 0);
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
    // 挖掘在分片线程里完成 等掉落物出现
    drain_by_peer(&mut state).await;
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
    // 每个已出生玩家只收方块更新
    assert_eq!(packet_ids(&commands), vec![UPDATE_BLOCK, UPDATE_BLOCK]);
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
    // 点击 (0,4,0) 的上表面 石头放到 (0,5,0)
    state.handle_game_packet(alice, &use_item(0, 4, 0, 1, 1));
    // 放置交给属分片去做 等结果回来
    drain_by_peer(&mut state).await;
    assert_eq!(state.block_at(0, 5, 0), 1);
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
    }
}

#[tokio::test]
async fn login_returns_initial_sequence() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    let commands = state.handle_game_packet(addr(), &login_packet());
    assert_eq!(commands.len(), 1);

    let packets = bodies(&commands[0]);
    // 8 个登录包 + 半径更新
    assert_eq!(packets.len(), 9);
    assert_eq!(packets[0][0], PLAY_STATUS);
    assert_eq!(packets[1][0], START_GAME);
    assert_eq!(packets[2][0], SET_TIME);
    assert_eq!(packets[3][0], SET_SPAWN_POSITION);
    assert_eq!(packets[4][0], UPDATE_ATTRIBUTES);
    assert_eq!(packets[5][0], SET_DIFFICULTY);
    assert_eq!(packets[6][0], SET_PLAYER_GAMETYPE);
    assert_eq!(packets[7][0], ADVENTURE_SETTINGS);
    assert_eq!(packets[8][0], CHUNK_RADIUS_UPDATE);
    assert_eq!(&packets[8][1..5], &DEFAULT_VIEW_DISTANCE.to_be_bytes());

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
    let packets = bodies(&commands[0]);

    assert_eq!(i32::from_be_bytes(packets[1][10..14].try_into().unwrap()), 1);
    assert_eq!(&packets[6][1..5], &1i32.to_be_bytes());
    let flags = i32::from_be_bytes(packets[7][1..5].try_into().unwrap());
    assert_ne!(flags & FLAG_ALLOW_FLIGHT, 0);
}

#[tokio::test]
async fn repeated_login_is_ignored() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    assert_eq!(state.handle_game_packet(addr(), &login_packet()).len(), 1);
    assert!(state.handle_game_packet(addr(), &login_packet()).is_empty());
}

// 区块由 region 线程按自己的周期推进 连续这么多轮空 tick 才认为真的没活了
const IDLE_POLLS: usize = 8;
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(10);

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
            } else {
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

    state.on_session_closed(&addr());
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
    let neighbour = crate::level::region::RegionId { x: -1, z: 0 };
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
        "mojor-idle-save-{}-{}.redb",
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
        crate::level::Level::new(),
        0,
        3,
    );
    let alice = peer(50021);
    let commands = state.handle_game_packet(alice, &login_packet());

    // 登录回包里带的半径就是配置值
    let mut radius_seen = false;
    for cmd in &commands {
        for packet in bodies(cmd) {
            if packet[0] == CHUNK_RADIUS_UPDATE {
                assert_eq!(&packet[1..5], &3i32.to_be_bytes());
                radius_seen = true;
            }
        }
    }
    assert!(radius_seen, "登录应下发半径");

    let (chunks, _) = drain(&mut state).await;
    // 半径 3 时下发圈还是完整 7x7 49 块 buffer 要更大视距才切角
    assert_eq!(chunks.len(), 49);
}

// 客户端要求更大的视距时按服务端配置截断
#[tokio::test]
async fn client_radius_is_capped_by_config() {
    let mut state = NetworkState::with_world(
        GAMEMODE_SURVIVAL,
        crate::level::Level::new(),
        0,
        2,
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
    // 出生前先同步背包 之后紧跟自己的玩家列表项
    assert_eq!(
        tail,
        vec![
            CONTAINER_SET_CONTENT,
            CONTAINER_SET_CONTENT,
            RESPAWN,
            PLAY_STATUS,
            PLAYER_LIST
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
    // 头部 14 字节之后是方块数组 索引 4 即区块内 (0, 4, 0)
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
            CONTAINER_SET_CONTENT,
            CONTAINER_SET_CONTENT,
            RESPAWN,
            PLAY_STATUS,
            PLAYER_LIST
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
    assert!(state.tick().await.is_empty());
}

// tick 产生的下行按目标地址聚合包 id
async fn drain_by_peer(state: &mut NetworkState) -> HashMap<SocketAddr, Vec<u8>> {
    let mut out: HashMap<SocketAddr, Vec<u8>> = HashMap::new();
    for cmd in &pump(state).await {
        let SendCmd::GamePackets { addr, bodies } = cmd;
        let entry = out.entry(*addr).or_default();
        for body in bodies {
            entry.push(body[0]);
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
    // 场上只有自己 只应有自己的列表项 没有实体
    assert_eq!(first[&alice].last(), Some(&PLAYER_LIST));
    assert!(!first[&alice].contains(&ADD_PLAYER));

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
    assert_eq!(text.message, "bob joined the game");

    let left = state.on_session_closed(&bob);
    let bodies = bodies_of(&left, alice);
    let notice = bodies
        .iter()
        .find(|body| body[0] == TEXT)
        .expect("离开时要通知其他玩家");
    assert_eq!(TextPacket::decode(notice).unwrap().message, "bob left the game");
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
        let SendCmd::GamePackets { addr, bodies } = cmd;
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

    let commands = state.on_session_closed(&bob);
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

// 斜杠命令在 global 层消费 不会走进聊天广播
#[tokio::test]
async fn slash_command_is_not_broadcast_as_chat() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

    let commands = state.handle_game_packet(addr(), &chat_packet("/time query"));
    let bodies = bodies_of(&commands, addr());
    assert_eq!(bodies.len(), 1, "命令只回给发起者一条");
    let text = TextPacket::decode(&bodies[0][..]).unwrap();
    assert_eq!(text.kind, TYPE_RAW);
    assert!(text.message.starts_with("Time is "), "{}", text.message);
}

// /time set 改世界时间并立刻向全体同步 SetTime
#[tokio::test]
async fn time_set_changes_world_time_and_syncs() {
    let mut state = NetworkState::new(GAMEMODE_SURVIVAL);
    state.handle_game_packet(addr(), &login_packet());

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
        "mojor-spawn-{}-{}.redb",
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
