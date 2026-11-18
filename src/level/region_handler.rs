// 玩家包在属主 region 线程里消费 对应 Folia 的 PacketProcessor
// 玩家状态 视野队列 广播展开都在这里完成 global region 只拿一份只读快照
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use bytes::Bytes;
use tracing::warn;

use crate::block::placement::{self, Context as PlacementContext};
use crate::entity::{
    PLAYER_HALF_WIDTH, PLAYER_HEIGHT, PlayerDirectory, PlayerState, RegionPlayer,
};
use crate::level::generator::blocks;
use crate::network::protocol::update_block::UpdateBlockPacket;
use crate::level::region::{REGION_BLOCKS, RegionId};
use crate::level::actor::{RegionMsg, RegionRegistry};
use crate::item::inventory::CraftingManager;
use crate::network::protocol::animate::AnimatePacket;
use crate::network::protocol::container::{
    ContainerClosePacket, ContainerSetSlotPacket, CraftingEventPacket, WINDOW_INVENTORY,
};
use crate::network::protocol::move_entity::{MoveEntityEntry, MoveEntityPacket};
use crate::network::protocol::move_player::{EYE_HEIGHT, MovePlayerPacket};
use crate::network::protocol::chunk_radius::{ChunkRadiusUpdatePacket, RequestChunkRadiusPacket};
use crate::network::protocol::entity_data::Metadata;
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::pid::{
    ANIMATE, CONTAINER_CLOSE, CONTAINER_SET_SLOT, CRAFTING_EVENT, ENTITY_EVENT, MOVE_PLAYER,
    PLAYER_ACTION, REMOVE_BLOCK, REQUEST_CHUNK_RADIUS, TEXT, USE_ITEM,
};
use crate::network::protocol::player_action::{
    ACTION_RELEASE_ITEM, ACTION_START_SNEAK, ACTION_START_SPRINT, ACTION_STOP_SNEAK,
    ACTION_STOP_SPRINT, PlayerActionPacket,
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
            &mut events,
        ),
        CRAFTING_EVENT => on_crafting_event(players, addr, packet, &mut commands),
        CONTAINER_SET_SLOT => {
            on_container_set_slot(players, addr, packet, &mut events);
        }
        CONTAINER_CLOSE => on_container_close(addr, packet, &mut commands, &mut events),
        TEXT => on_text(players, directory, addr, packet, view_distance, &mut commands),
        PLAYER_ACTION => {
            on_player_action(
                players, directory, addr, packet, view_distance, &mut commands, &mut events,
            );
        }
        ENTITY_EVENT => {
            on_entity_event(players, directory, addr, packet, view_distance, &mut commands);
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
        Err(_) => {
            return;
        }
    };
    // 客户端发的是眼睛位置 换成脚部坐标
    let position = mv.feet_position();

    // 传送落点还没被客户端确认时 在路上晃的旧坐标不能把玩家拉回去
    // 照搬 Nukkit handleDataPacket 的 forceMovement 距离校验 每次偏差都重发一次落点
    if let Some(forced) = player.force_position {
        let dx = position.0 - forced.0;
        let dy = position.1 - forced.1;
        let dz = position.2 - forced.2;
        if dx * dx + dy * dy + dz * dz > 0.1 {
            let body = MovePlayerPacket::teleport(0, forced, mv.yaw, mv.pitch).encode();
            commands.push(reply(addr, vec![body]));
            return;
        }
    }

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
        // 客户端已经站到落点上了 传送纠正结束
        player.force_position = None;
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
            // 回滚不能只把玩家塞回来: 坐标已经跨出本分片了
            // 归属还在本分片会让区块队列越界 放置校验也按本分片边界算
            // 所以把坐标夹回内侧发一次传送纠正 客户端跟着回来
            let mut moved = moved;
            let safe = clamp_into(id, moved.state.position);
            moved.state.position = safe;
            moved.view.update(safe, view_distance);
            // 客户端确认站位前不采信它的移动包
            moved.force_position = Some(safe);
            let body =
                MovePlayerPacket::teleport(0, safe, moved.state.yaw, moved.state.pitch).encode();
            commands.push(reply(addr, vec![body]));
            players.insert(addr, moved);
        }
    }
}

// 把坐标夹回指定分片内侧 留 1 格余量 免得下一包又立刻判成跨区
// 只在迁移回滚时用: 目标分片不可用 玩家必须留在原分片里
fn clamp_into(id: RegionId, position: (f32, f32, f32)) -> (f32, f32, f32) {
    const MARGIN: f32 = 1.0;
    let min_x = (id.x * REGION_BLOCKS) as f32 + MARGIN;
    let max_x = ((id.x + 1) * REGION_BLOCKS) as f32 - MARGIN;
    let min_z = (id.z * REGION_BLOCKS) as f32 + MARGIN;
    let max_z = ((id.z + 1) * REGION_BLOCKS) as f32 - MARGIN;
    (
        position.0.clamp(min_x, max_x),
        position.1,
        position.2.clamp(min_z, max_z),
    )
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
        Err(_) => {
            return;
        }
    };
    if !within_reach(player.state.position, dig.x, dig.y, dig.z) {
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

// 客户端按服务端下发的配方表算好产物后上报 服务端校验扣料给产物
// 0.14 的合成由客户端主导 这里只认配方表里存在的产物 挡住凭空造物
fn on_crafting_event(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    addr: SocketAddr,
    packet: &[u8],
    commands: &mut Vec<SendCmd>,
) {
    let Ok(event) = CraftingEventPacket::decode(&packet[1..]) else {
        return;
    };
    let Some(player) = players.get_mut(&addr) else {
        return;
    };
    let manager = CraftingManager::new();
    let reported = event.output.iter().find(|s| !s.is_empty()).copied();

    // 优先按 CraftingData 下发的配方 UUID 精确校验: 产物逐字段一致 + 材料多重集合匹配
    // 客户端某些版本回传的 UUID/网格不是服务端这套 命中失败时退回宽松路径 别把合成改坏
    let strict = manager.recipe_by_uuid(&event.uuid).filter(|(expected, ingredients)| {
        if reported != Some(*expected) {
            return false;
        }
        let inputs: Vec<&crate::network::protocol::item::ItemStack> = event
            .input
            .iter()
            .filter(|s| !s.is_empty())
            .collect();
        if inputs.len() != ingredients.len() {
            return false;
        }
        let mut used = vec![false; inputs.len()];
        ingredients.iter().all(|need| {
            inputs
                .iter()
                .enumerate()
                .any(|(i, got)| !used[i] && need.matches(got) && {
                    used[i] = true;
                    true
                })
        })
    });

    if let Some((expected, ingredients)) = strict {
        for need in &ingredients {
            player.state.consume_item(need.id);
        }
        player.state.add_item(expected);
    } else if let Some(result) = reported.filter(|r| {
        r.id > 0
            && r.id <= 512
            && r.damage >= 0
            && (manager.shaped.iter().any(|x| x.result.id == r.id)
                || manager.shapeless.iter().any(|x| x.result.id == r.id)
                || manager.furnace.iter().any(|x| x.result.id == r.id))
    }) {
        // 宽松兜底: 只认产物是配方表里有的 按客户端上报的 input 扣料
        for stack in &event.input {
            if !stack.is_empty() {
                for _ in 0..stack.count.min(64) {
                    player.state.consume_item(stack.id);
                }
            }
        }
        player.state.add_item(result);
    }
    // 无论成没成 都把服务端背包推回去 客户端本地预测与服务端对齐
    commands.push(reply(addr, vec![player.state.inventory_packet().encode()]));
}

// 客户端拖动背包/合成网格里的物品 同步服务端槽位 否则放置时按旧背包扣料
fn on_container_set_slot(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    addr: SocketAddr,
    packet: &[u8],
    events: &mut Vec<crate::level::actor::RegionEvent>,
) {
    let Ok(slot) = ContainerSetSlotPacket::decode(&packet[1..]) else {
        return;
    };
    if slot.slot < 0 {
        return;
    }
    if slot.window_id != WINDOW_INVENTORY {
        // 容器窗口交给 global 的容器注册表 关窗后仍保留内容
        events.push(crate::level::actor::RegionEvent::ContainerSlot {
            addr,
            window_id: slot.window_id,
            slot: slot.slot as usize,
            item: slot.item,
        });
        return;
    }
    let Some(player) = players.get_mut(&addr) else {
        return;
    };
    let index = slot.slot as usize;
    if index < player.state.inventory.len() {
        // 客户端上报的槽位只接受合法物品 越界 id 会让它自己渲染背包时崩溃
        player.state.inventory[index] = slot
            .item
            .filter(|item| item.id > 0 && item.id <= 512 && item.damage >= 0);
    }
}

// 客户端关窗口 回一个确认并通知 global 解除关联
fn on_container_close(
    addr: SocketAddr,
    packet: &[u8],
    commands: &mut Vec<SendCmd>,
    events: &mut Vec<crate::level::actor::RegionEvent>,
) {
    let Some(&window_id) = packet.get(2) else {
        return;
    };
    commands.push(reply(
        addr,
        vec![ContainerClosePacket { window_id }.encode()],
    ));
    events.push(crate::level::actor::RegionEvent::ContainerClosed { addr });
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
    events: &mut Vec<crate::level::actor::RegionEvent>,
) {
    let (position, yaw, pitch) = match players.get(&addr) {
        Some(player) if player.state.spawned => (
            player.state.position,
            player.state.yaw,
            player.state.pitch,
        ),
        _ => return,
    };

    // 协议号取自该玩家的握手结果 0.14.0~0.14.2 的包没有 slot 字段 需按版本解析
    let protocol = players.get(&addr).map(|p| p.state.protocol).unwrap_or(
        crate::network::protocol::pid::CURRENT_PROTOCOL,
    );
    let use_item = match UseItemPacket::decode(&packet[1..], protocol) {
        Ok(item) => item,
        Err(_) => {
            return;
        }
    };
    // 协议 70 的 slot 字段是绝对背包槽位 更新手持槽供耐久/功能物品使用
    if use_item.slot >= 0 && (use_item.slot as usize) < 36 {
        if let Some(player) = players.get_mut(&addr) {
            player.state.held_slot = use_item.slot as usize;
        }
    }
    // 空手/手持点方块都先交给属主分片做交互判定: 门开关 + 容器 UI
    // Nukkit Level::useItemOn 里 target.onActivate 优先于放置
    // 交互目标是被点的那一格本身(可能点门上半) 不是放置用的 target_block
    // 潜行时不触发 跨线程拿不到结果 所以放置照常走 分片各自静默处理非交互方块
    if use_item.face <= 5 && within_reach(position, use_item.x, use_item.y, use_item.z) {
        let sneaking = players.get(&addr).is_some_and(|p| p.state.sneaking);
        if !sneaking {
            let owner = RegionId::of_block(use_item.x, use_item.z);
            let (x, y, z) = (use_item.x, use_item.y, use_item.z);
            post_to(
                registry,
                tx,
                id,
                owner,
                RegionMsg::ToggleDoorFor { addr, x, y, z },
            );
            post_to(
                registry,
                tx,
                id,
                owner,
                RegionMsg::OpenContainerFor { addr, x, y, z },
            );
        }
    }

    if use_item.item_id <= 0 {
        return;
    }

    // 投掷类物品(雪球/鸡蛋/末影珍珠)使用即抛出 不放置方块也不需要目标格
    // 方向取玩家朝向 起点抬到眼睛高度并前推一格 免得生成在玩家身体里
    // 箭要靠弓发射(见 ACTION_RELEASE_ITEM) 直接使用无效
    let thrown = if matches!(
        use_item.item_id,
        id if id == crate::item::ids::SNOWBALL
            || id == crate::item::ids::EGG
            || id == crate::item::ids::ENDER_PEARL
    ) {
        crate::entity::projectile::kind_from_item(use_item.item_id)
    } else {
        None
    };
    if let Some(kind) = thrown {
        // 末影珍珠不用点击方块也能扔 这里只接受合法朝向的玩家
        let direction = crate::entity::projectile::direction_from_rotation(yaw, pitch);
        let origin = (
            position.0,
            position.1 + EYE_HEIGHT,
            position.2,
        );
        let start = (
            origin.0 + direction.0 * 0.6,
            origin.1 + direction.1 * 0.6,
            origin.2 + direction.2 * 0.6,
        );
        // 原版初速: 箭由蓄力决定这里取满蓄力 雪球/蛋/珍珠固定 1.5
        let speed = match kind {
            crate::entity::projectile::ProjectileKind::Arrow => 2.5,
            _ => 1.5,
        };
        // 生存扣一件 创造不扣
        if gamemode & GAMEMODE_CREATIVE == 0 {
            let Some(player) = players.get_mut(&addr) else {
                return;
            };
            if !player.state.consume_item(use_item.item_id) {
                return;
            }
            commands.push(reply(
                addr,
                vec![player.state.inventory_packet().encode()],
            ));
        }
        events.push(crate::level::actor::RegionEvent::LaunchProjectile {
            kind,
            position: start,
            direction,
            speed,
        });
        return;
    }

    let Some((x, y, z)) = use_item.target_block() else {
        return;
    };
    if !within_reach(position, x, y, z) {
        return;
    }

    // 功能物品(桶/打火石/锄头/骨粉)走单独分支 由属主分片读世界后判定并应用
    // click 是被点方块 use_item 的 x/y/z target 是点击面相邻格 (x,y,z)
    // 物品实例从背包取(带 damage) 保证桶的 damage 状态正确
    if crate::item::functional::is_functional(use_item.item_id, use_item.item_damage) {
        let held = players
            .get(&addr)
            .and_then(|p| p.state.held_item())
            .unwrap_or(ItemStack {
                id: use_item.item_id,
                count: 1,
                damage: use_item.item_damage,
            });
        let click = (use_item.x, use_item.y, use_item.z);
        // 协议 45/46/60 没有 slot 字段(-1) 一律用服务端记录的 held_slot
        // 协议 70 上面已按 slot 更新过 held_slot 这里统一取它 免得落到槽 0
        let slot = players
            .get(&addr)
            .map(|p| p.state.held_slot)
            .unwrap_or(use_item.slot.max(0) as usize);
        let owner = RegionId::of_block(click.0, click.2);
        post_to(registry, tx, id, owner, RegionMsg::UseItemOn {
            addr,
            item: held,
            click,
            target: (x, y, z),
            slot,
        });
        return;
    }

    // 手持物品 -> 实际放的方块 门/床/告示牌 的物品 id 与方块 id 不是一个数
    let context = PlacementContext {
        item_id: use_item.item_id,
        item_meta: use_item.item_damage,
        face: use_item.face,
        yaw,
        fy: use_item.fy,
    };
    let Some(cells) = placement::resolve(&context) else {
        return;
    };

    // 固体方块不能塞进自己身体里 原版客户端也会拦这一步 否则就是站在方块里穿模
    let blocked: Vec<(i32, i32, i32)> = cells
        .iter_abs(x, y, z)
        .filter(|&(cx, cy, cz, cell)| blocks::is_solid(cell.id) && covers_player(cx, cy, cz, position))
        .map(|(cx, cy, cz, _)| (cx, cy, cz))
        .collect();
    if !blocked.is_empty() {
        // 客户端已经在本地把方块摆上了 把这几格推回空气
        let bodies = blocked
            .iter()
            .map(|&(bx, by, bz)| UpdateBlockPacket::single(bx, by, bz, 0, 0).encode())
            .collect();
        commands.push(reply(addr, bodies));
        return;
    }

    // 生存扣的是手持物品 不是放下去的方块 id
    if gamemode & GAMEMODE_CREATIVE == 0 {
        let Some(player) = players.get_mut(&addr) else {
            return;
        };
        if !player.state.consume_item(use_item.item_id) {
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
        cells,
        item: use_item.item_id,
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
    _view_distance: i32,
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
        Err(_) => {
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
        // 聊天发给全体已进入服务器的玩家 不限于视野 照搬 Nukkit 的全局广播
        for (peer, _) in directory.spawned() {
            commands.push(SendCmd::GamePackets {
                addr: peer,
                bodies: vec![body.clone()],
            });
        }
        // 玩家聊天同步一份到服务端控制台 照搬 Mojang 的 "<name> message" 日志格式
        commands.push(SendCmd::Console(format!("<{}> {}", player.state.username, line)));
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
    events: &mut Vec<crate::level::actor::RegionEvent>,
) {
    let action = match PlayerActionPacket::decode(&packet[1..]) {
        Ok(action) => action,
        Err(_) => {
            return;
        }
    };
    let Some(player) = players.get_mut(&addr) else {
        return;
    };
    if !player.state.spawned {
        return;
    }
    // 松开弓弦: 手持弓且背包里还有箭才算一次发射 方向取玩家朝向
    if action.action == ACTION_RELEASE_ITEM {
        if player
            .state
            .held_item()
            .is_some_and(|item| item.id == crate::item::ids::BOW)
        {
            let direction =
                crate::entity::projectile::direction_from_rotation(player.state.yaw, player.state.pitch);
            let origin = (
                player.state.position.0,
                player.state.position.1 + EYE_HEIGHT,
                player.state.position.2,
            );
            let start = (
                origin.0 + direction.0 * 0.6,
                origin.1 + direction.1 * 0.6,
                origin.2 + direction.2 * 0.6,
            );
            // 生存消耗一支箭 创造不扣
            let can_shoot = if player.state.gamemode & GAMEMODE_CREATIVE != 0 {
                true
            } else {
                player.state.consume_item(crate::item::ids::ARROW)
            };
            if can_shoot {
                events.push(crate::level::actor::RegionEvent::LaunchProjectile {
                    kind: crate::entity::projectile::ProjectileKind::Arrow,
                    position: start,
                    direction,
                    speed: 2.5,
                });
                if player.state.gamemode & GAMEMODE_CREATIVE == 0 {
                    commands.push(reply(
                        addr,
                        vec![player.state.inventory_packet().encode()],
                    ));
                }
            }
        }
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

// 客户端吃完食物后上报 EntityEvent(USE_ITEM=9) 这里消耗手中食物并应用营养
// 与 ITX Player::handleDataPacket 的 EntityEventPacket case 9 -> eatFoodInHand 对齐
fn on_entity_event(
    players: &mut HashMap<SocketAddr, RegionPlayer>,
    directory: &Arc<PlayerDirectory>,
    addr: SocketAddr,
    packet: &[u8],
    view_distance: i32,
    commands: &mut Vec<SendCmd>,
) {
    let event = match crate::network::protocol::entity_event::EntityEventPacket::decode(&packet[1..]) {
        Ok(e) => e,
        Err(_) => return,
    };
    if event.event != crate::network::protocol::entity_event::ENTITY_EVENT_USE_ITEM {
        return;
    }
    let Some(player) = players.get_mut(&addr) else {
        return;
    };
    if !player.state.spawned || player.state.gamemode & GAMEMODE_CREATIVE != 0 {
        return;
    }
    if player.state.eat_cooldown > 0 {
        return;
    }

    if let Some((own, broad)) = crate::item::behavior::eat(&mut player.state, 0) {
        // ITX eatCoolDown 2 秒 = 40 tick
        player.state.eat_cooldown = 40;
        commands.push(reply(addr, own));
        let center = player.state.position;
        commands.extend(broadcast(directory, center, view_distance, &broad, Some(addr)));
    }
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
        Err(_) => {
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
        Err(_) => {},
    }
}

// 交互距离限制 超过四格不认
// 这一格和玩家碰撞盒(0.6 x 1.8)有没有重叠 三维都要重叠才算
fn covers_player(x: i32, y: i32, z: i32, position: (f32, f32, f32)) -> bool {
    let (px, py, pz) = position;
    let overlap_x = (x as f32) < px + PLAYER_HALF_WIDTH && (x + 1) as f32 > px - PLAYER_HALF_WIDTH;
    let overlap_y = (y as f32) < py + PLAYER_HEIGHT && (y + 1) as f32 > py;
    let overlap_z = (z as f32) < pz + PLAYER_HALF_WIDTH && (z + 1) as f32 > pz - PLAYER_HALF_WIDTH;
    overlap_x && overlap_y && overlap_z
}

fn within_reach(position: (f32, f32, f32), x: i32, y: i32, z: i32) -> bool {
    let dx = position.0 - x as f32;
    let dy = position.1 - y as f32;
    let dz = position.2 - z as f32;
    dx * dx + dy * dy + dz * dz <= 64.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_pulls_back_into_region() {
        let id = RegionId { x: 0, z: 0 };
        // 已经跨到 x=512 的那一侧 要被夹回 511 附近
        let (x, y, z) = clamp_into(id, (520.5, 64.0, 3.0));
        assert!(x < 512.0 && x >= 511.0, "x 没夹回来: {x}");
        assert_eq!(y, 64.0, "y 不该动");
        assert_eq!(z, 3.0);
        assert_eq!(RegionId::of_block(x.floor() as i32, z.floor() as i32), id);

        // 已经在分片内部就不动
        let inside = clamp_into(id, (100.5, 64.0, 100.5));
        assert_eq!(inside, (100.5, 64.0, 100.5));
    }

    #[test]
    fn clamp_handles_negative_regions() {
        // 负分片边界要按位运算算 不能直接乘
        let id = RegionId { x: -1, z: -1 };
        let (x, _, z) = clamp_into(id, (-0.5, 64.0, -0.5));
        assert!(x < 0.0 && x >= -1.0, "x 没夹回来: {x}");
        assert!(z < 0.0 && z >= -1.0, "z 没夹回来: {z}");
        assert_eq!(RegionId::of_block(x.floor() as i32, z.floor() as i32), id);
    }
}
