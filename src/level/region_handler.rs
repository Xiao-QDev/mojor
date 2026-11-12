// 玩家包在属主 region 线程里消费 对应 Folia 的 PacketProcessor
// 玩家状态 视野队列 广播展开都在这里完成 global region 只拿一份只读快照
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use bytes::Bytes;
use tracing::warn;

use crate::entity::{PlayerDirectory, PlayerState, RegionPlayer};
use crate::level::region::RegionId;
use crate::level::actor::{RegionMsg, RegionRegistry};
use crate::network::protocol::animate::AnimatePacket;
use crate::network::protocol::move_entity::{MoveEntityEntry, MoveEntityPacket};
use crate::network::protocol::move_player::{EYE_HEIGHT, MovePlayerPacket};
use crate::network::protocol::chunk_radius::{ChunkRadiusUpdatePacket, RequestChunkRadiusPacket};
use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::pid::{
    ANIMATE, MOVE_PLAYER, PLAYER_ACTION, REMOVE_BLOCK, REQUEST_CHUNK_RADIUS, TEXT, USE_ITEM,
};
use crate::network::protocol::player_action::{
    ACTION_START_SNEAK, ACTION_START_SPRINT, ACTION_STOP_SNEAK, ACTION_STOP_SPRINT,
    PlayerActionPacket,
};
use crate::network::protocol::remove_block::RemoveBlockPacket;
use crate::network::protocol::set_entity_data::SetEntityDataPacket;
use crate::network::protocol::text::{MAX_MESSAGE_LEN, TYPE_CHAT, TextPacket};
use crate::network::protocol::use_item::UseItemPacket;
use crate::network::{GAMEMODE_CREATIVE, SendCmd};

// 一次包处理的结果 下行直接回给 global region 玩家状态用来刷新它的镜像
pub(crate) struct PacketResult {
    pub(crate) commands: Vec<SendCmd>,
    pub(crate) players: Vec<(SocketAddr, PlayerState)>,
    // 需要 global region 处理的状态变更 由分片转发到 outbound
    pub(crate) events: Vec<crate::level::actor::RegionEvent>,
}

impl PacketResult {
    pub(crate) fn empty(players: &HashMap<SocketAddr, RegionPlayer>) -> Self {
        Self {
            commands: Vec::new(),
            players: snapshot(players),
            events: Vec::new(),
        }
    }
}

fn snapshot(players: &HashMap<SocketAddr, RegionPlayer>) -> Vec<(SocketAddr, PlayerState)> {
    players
        .iter()
        .map(|(addr, player)| (*addr, player.state.clone()))
        .collect()
}

// 广播按玩家目录展开 不读别的 region 的玩家状态
// 只投给以 center 为中心视野内的玩家 skip 用于不发给来源本人
fn broadcast(
    directory: &Arc<PlayerDirectory>,
    center: (f32, f32, f32),
    view_distance: i32,
    bodies: &[Bytes],
    skip: Option<SocketAddr>,
) -> Vec<SendCmd> {
    let radius = (view_distance * 16) as f32;
    directory
        .nearby(center, radius)
        .into_iter()
        .filter(|(addr, _)| Some(*addr) != skip)
        .map(|(addr, _)| SendCmd::GamePackets {
            addr,
            bodies: bodies.to_vec(),
        })
        .collect()
}

fn reply(addr: SocketAddr, bodies: Vec<Bytes>) -> SendCmd {
    SendCmd::GamePackets { addr, bodies }
}

pub(crate) fn handle_packet(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    registry: &Arc<RegionRegistry>,
    tx: &Sender<RegionMsg>,
    id: RegionId,
    addr: SocketAddr,
    packet: &[u8],
    gamemode: u8,
    view_distance: i32,
) -> PacketResult {
    let Some(player) = players.get(&addr) else {
        return PacketResult::empty(players);
    };
    if !player.state.logged_in {
        return PacketResult::empty(players);
    }

    let mut commands = Vec::new();
    let mut events = Vec::new();
    match packet[1] {
        MOVE_PLAYER => on_move_player(
            players,
            directory,
            registry,
            tx,
            id,
            addr,
            packet,
            view_distance,
            &mut commands,
            &mut events,
        ),
        REMOVE_BLOCK => on_remove_block(players, registry, tx, id, addr, packet, &mut commands),
        USE_ITEM => on_use_item(
            players,
            registry,
            tx,
            id,
            addr,
            packet,
            gamemode,
            &mut commands,
        ),
        TEXT => on_text(players, directory, addr, packet, view_distance, &mut commands),
        PLAYER_ACTION => {
            on_player_action(players, directory, addr, packet, view_distance, &mut commands);
        }
        ANIMATE => on_animate(players, directory, addr, packet, view_distance, &mut commands),
        REQUEST_CHUNK_RADIUS => {
            on_request_radius(addr, packet, view_distance, &mut commands);
        }
        _ => {}
    }

    PacketResult {
        commands,
        players: snapshot(players),
        events,
    }
}

// 移动包在属主分片里消费 位置 视野队列 广播与跨区迁移都在本线程
fn on_move_player(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    registry: &Arc<RegionRegistry>,
    tx: &Sender<RegionMsg>,
    id: RegionId,
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
    events: &mut Vec<crate::level::actor::RegionEvent>,
) {
    let Some(player) = players.get(&addr) else {
        return;
    };
    if !player.state.logged_in {
        return;
    }
    let mv = match MovePlayerPacket::decode(&packet[1..]) {
        Ok(mv) => mv,
        Err(e) => {
            warn!(%addr, error = %e, "移动包解析失败");
            return;
        }
    };
    // 客户端发的是眼睛位置 换成脚部坐标
    let position = mv.feet_position();
    let old_region = RegionId::of_block(
        player.state.position.0.floor() as i32,
        player.state.position.2.floor() as i32,
    );
    let new_region = RegionId::of_block(position.0.floor() as i32, position.2.floor() as i32);

    {
        let player = players.get_mut(&addr).expect("刚刚查过");
        player.state.position = position;
        player.state.yaw = mv.yaw;
        player.state.pitch = mv.pitch;
        player.view.update(position, view_distance);
    }

    // 移动同步给附近的其他玩家
    let player = &players[&addr];
    if player.state.spawned {
        let body = MoveEntityPacket {
            entries: vec![MoveEntityEntry {
                eid: player.state.eid,
                // 0.14 客户端把移动包里的 y 当眼睛位置
                position: (position.0, position.1 + EYE_HEIGHT, position.2),
                yaw: mv.yaw,
                head_yaw: mv.yaw,
                pitch: mv.pitch,
            }],
        }
        .encode();
        commands.extend(broadcast(directory, position, view_distance, &[body], Some(addr)));
    }

    // 跨区走两段式 只投递不等待 目标分片自己接管并重建视野
    if old_region != new_region {
        let moved = players.remove(&addr).expect("刚刚查过");
        let target = if new_region == id {
            Some(tx.clone())
        } else {
            registry.lookup(new_region)
        };
        let delivered = target.is_some_and(|target| {
            target
                .send(RegionMsg::Transfer {
                    addr,
                    player: Box::new(moved.clone()),
                    view_distance,
                })
                .is_ok()
        });
        if delivered {
            events.push(crate::level::actor::RegionEvent::PlayerRegion {
                addr,
                region: new_region,
                state: Box::new(moved.state.clone()),
            });
        } else {
            warn!(
                region_x = id.x,
                region_z = id.z,
                target_x = new_region.x,
                target_z = new_region.z,
                "目标分片不可用 玩家留在本分片"
            );
            players.insert(addr, moved);
        }
    }
}

// 挖方块 校验距离后交给方块属主分片去挖
fn on_remove_block(
    players: &HashMap<SocketAddr, RegionPlayer>,
    registry: &Arc<RegionRegistry>,
    tx: &Sender<RegionMsg>,
    id: RegionId,
    addr: SocketAddr,
    packet: &[u8],
    _commands: &mut Vec<SendCmd>,
) {
    let Some(player) = players.get(&addr) else {
        return;
    };
    if !player.state.spawned {
        return;
    }
    let dig = match RemoveBlockPacket::decode(&packet[1..]) {
        Ok(dig) => dig,
        Err(e) => {
            warn!(%addr, error = %e, "挖方块包解析失败");
            return;
        }
    };
    if !within_reach(player.state.position, dig.x, dig.y, dig.z) {
        warn!(%addr, x = dig.x, y = dig.y, z = dig.z, "挖方块距离过远 已忽略");
        return;
    }

    let msg = RegionMsg::BreakBlockFor {
        addr,
        x: dig.x,
        y: dig.y,
        z: dig.z,
    };
    post_to(registry, tx, id, RegionId::of_block(dig.x, dig.z), msg);
}

// 放方块 扣背包后交给方块属主分片判定并写入
fn on_use_item(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    registry: &Arc<RegionRegistry>,
    tx: &Sender<RegionMsg>,
    id: RegionId,
    addr: SocketAddr,
    packet: &[u8],
    gamemode: u8,
    commands: &mut Vec<SendCmd>,
) {
    let position = match players.get(&addr) {
        Some(player) if player.state.spawned => player.state.position,
        _ => return,
    };

    let use_item = match UseItemPacket::decode(&packet[1..]) {
        Ok(item) => item,
        Err(e) => {
            warn!(%addr, error = %e, "使用物品包解析失败");
            return;
        }
    };
    if use_item.item_id <= 0 {
        return;
    }
    let Some((x, y, z)) = use_item.target_block() else {
        return;
    };
    if !within_reach(position, x, y, z) {
        return;
    }

    let block_id = use_item.item_id as u8;
    if gamemode & GAMEMODE_CREATIVE == 0 {
        let Some(player) = players.get_mut(&addr) else {
            return;
        };
        if !player.state.consume_item(block_id) {
            return;
        }
        commands.push(reply(
            addr,
            vec![player.state.inventory_packet().encode()],
        ));
    }

    let msg = RegionMsg::PlaceBlockFor {
        addr,
        click: (use_item.x, use_item.y, use_item.z),
        target: (x, y, z),
        id: block_id,
    };
    post_to(registry, tx, id, RegionId::of_block(x, z), msg);
}

// 本分片就地投递 跨分片只投不等待
fn post_to(
    registry: &Arc<RegionRegistry>,
    tx: &Sender<RegionMsg>,
    id: RegionId,
    owner: RegionId,
    msg: RegionMsg,
) {
    if owner == id {
        let _ = tx.send(msg);
    } else if let Some(target) = registry.lookup(owner) {
        let _ = target.send(msg);
    } else {
        warn!(
            region_x = id.x,
            region_z = id.z,
            owner_x = owner.x,
            owner_z = owner.z,
            "方块属主分片不可用 已丢弃交互"
        );
    }
}

// 聊天按行拆分 来源以服务端记录的用户名为准 只投给视野内的玩家
fn on_text(
    players: &HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
) {
    let Some(player) = players.get(&addr) else {
        return;
    };
    if !player.state.spawned {
        return;
    }
    let text = match TextPacket::decode(&packet[1..]) {
        Ok(text) => text,
        Err(e) => {
            warn!(%addr, error = %e, "聊天包解析失败");
            return;
        }
    };
    if text.kind != TYPE_CHAT {
        return;
    }
    for line in text.message.split('\n') {
        let line = line.trim();
        if line.is_empty() || line.chars().count() > MAX_MESSAGE_LEN {
            continue;
        }
        let body = TextPacket::chat(&player.state.username, line).encode();
        let center = player.state.position;
        commands.extend(broadcast(directory, center, view_distance, &[body], None));
    }
}

// 潜行与疾跑切换后同步给视野内的其他玩家
fn on_player_action(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
) {
    let action = match PlayerActionPacket::decode(&packet[1..]) {
        Ok(action) => action,
        Err(e) => {
            warn!(%addr, error = %e, "玩家动作包解析失败");
            return;
        }
    };
    let Some(player) = players.get_mut(&addr) else {
        return;
    };
    if !player.state.spawned {
        return;
    }
    // 客户端上报的 eid 不可信 一律用服务端分配的值
    let changed = match action.action {
        ACTION_START_SNEAK if !player.state.sneaking => {
            player.state.sneaking = true;
            true
        }
        ACTION_STOP_SNEAK if player.state.sneaking => {
            player.state.sneaking = false;
            true
        }
        ACTION_START_SPRINT if !player.state.sprinting => {
            player.state.sprinting = true;
            true
        }
        ACTION_STOP_SPRINT if player.state.sprinting => {
            player.state.sprinting = false;
            true
        }
        _ => false,
    };
    if !changed {
        return;
    }
    let body = SetEntityDataPacket {
        eid: player.state.eid,
        metadata: Metadata::player_with_flags(
            &player.state.username,
            player.state.entity_flags(),
        ),
    }
    .encode();
    let center = player.state.position;
    commands.extend(broadcast(directory, center, view_distance, &[body], Some(addr)));
}

// 挥手动画转发给视野内的其他玩家
fn on_animate(
    players: &HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
) {
    let animate = match AnimatePacket::decode(&packet[1..]) {
        Ok(animate) => animate,
        Err(e) => {
            warn!(%addr, error = %e, "动画包解析失败");
            return;
        }
    };
    let Some(player) = players.get(&addr) else {
        return;
    };
    if !player.state.spawned {
        return;
    }
    let body = AnimatePacket {
        action: animate.action,
        eid: player.state.eid,
    }
    .encode();
    let center = player.state.position;
    commands.extend(broadcast(directory, center, view_distance, &[body], Some(addr)));
}

fn on_request_radius(
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
) {
    match RequestChunkRadiusPacket::decode(&packet[1..]) {
        Ok(request) => {
            let radius = request.radius.clamp(1, view_distance);
            commands.push(reply(
                addr,
                vec![ChunkRadiusUpdatePacket { radius }.encode()],
            ));
        }
        Err(e) => warn!(%addr, error = %e, "区块半径请求解析失败"),
    }
}

// 交互距离限制 超过四格不认
fn within_reach(position: (f32, f32, f32), x: i32, y: i32, z: i32) -> bool {
    let dx = position.0 - x as f32;
    let dy = position.1 - y as f32;
    let dz = position.2 - z as f32;
    dx * dx + dy * dy + dz * dz <= 64.0
}
