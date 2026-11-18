use std::net::SocketAddr;

use tracing::warn;

use crate::entity::{PlayerData, RegionPlayer, assign_uuid};
use crate::level::actor::RegionEvent;
use crate::level::region::RegionId;
use crate::network::protocol::container::ContainerOpenPacket;
use crate::network::protocol::container_set_content::ContainerSetContentPacket;
use crate::network::protocol::entity_event::{
    ENTITY_EVENT_HURT, EntityEventPacket, SetHealthPacket,
};
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::level_event::LevelEventPacket;
use crate::network::protocol::login::LoginPacket;
use crate::network::protocol::update_block::UpdateBlockPacket;

use super::broadcast::reply;
use super::login::{client_protocol, login_sequence, protocol_accepted};
use super::{GAMEMODE_CREATIVE, NetworkState, SendCmd};

impl NetworkState {
    pub(crate) fn on_login(&mut self, addr: SocketAddr, packet: &[u8], out: &mut Vec<SendCmd>) {
        if self.players.get(&addr).is_some_and(|state| state.logged_in) {
            return;
        }

        let login = match LoginPacket::decode(&packet[1..]) {
            Ok(login) => login,
            Err(_) => {
                return;
            }
        };

        let eid = self.next_eid;
        self.next_eid += 1;

        let accepted = protocol_accepted(&login);
        let (spawn_block, spawn_position) = self.spawn_point();
        let bodies = login_sequence(
            &login,
            self.gamemode,
            self.difficulty,
            self.seed,
            spawn_block,
            spawn_position,
            &self.world_time,
        );
        let view_distance = self.view_distance();

        // 玩家身份由服务端分配 存档按 uuid 落到 playerdata/<uuid>.dat 与 Java 版一致
        // 同名的另一台设备带来的是另一个 uuid 只能拿到自己那份 读不到别人的背包
        let uuid = assign_uuid(login.client_uuid, &login.username);
        // 有档就恢复位置朝向背包 没有就停在出生点
        let saved = self
            .level
            .data_dir()
            .and_then(|dir| PlayerData::load(dir, uuid, &login.username));

        // 记下客户端协议版本 供 region 线程按版本解析 UseItem 等有差异的包
        // 必须放在移动 login.username / login.skin 之前
        let client_protocol = client_protocol(&login);
        let state = self.players.entry(addr).or_default();
        state.logged_in = true;
        state.eid = eid;
        state.uuid = uuid;
        state.username = login.username;
        state.skin = login.skin;
        state.protocol = client_protocol;
        state.gamemode = self.gamemode;
        state.difficulty = self.difficulty;
        state.position = spawn_position;
        if let Some(data) = saved {
            data.apply_to(state);
        }
        // 区块队列由 region 线程自己推进 这里只把视图交过去
        state.spawn_pending = accepted;
        let player_position = state.position;
        let snapshot = state.clone();
        // 登录完成 放开这条连接的下行 旧会话在途的残包在此之前已被丢弃
        out.push(SendCmd::SessionReady { addr });
        out.push(reply(addr, bodies));

        if accepted {
            self.player_index.insert(
                addr,
                RegionId::of_block(
                    player_position.0.floor() as i32,
                    player_position.2.floor() as i32,
                ),
            );
            // 手电筒点亮 出生区块当距离场的源 照搬 DistanceManager.addPlayer
            let chunk = (
                player_position.0.floor() as i32 >> 4,
                player_position.2.floor() as i32 >> 4,
            );
            self.chunk_tracker.add_player(addr, chunk);
            self.level
                .join(addr, RegionPlayer::new(snapshot, view_distance));
            // 进入即存一次 登录时就写出 playerdata/<uuid>.dat
            // 与下线存档 停机兜底(save_all_players) 三者独立互不依赖
            if let Some(dir) = self.level.data_dir() {
                let data = PlayerData::from_state(state);
                if let Err(e) = data.save(dir, state.uuid) {
                    warn!("保存玩家数据失败 {}: {e}", state.username);
                }
            }
        }
    }

    // region 做完了世界改动 结果在这里落地
    // global region 只管玩家目录 背包与下行 实体本身归分片
    pub(crate) fn on_region_event(&mut self, event: RegionEvent, out: &mut Vec<SendCmd>) {
        match event {
            RegionEvent::BlockBroken {
                addr,
                x,
                y,
                z,
                id,
                meta,
                ok,
                paired,
            } => {
                if !ok {
                    // 没让挖(基岩) 客户端可能已经把这格本地删了 只发给他本人补回去
                    // 空气没什么可补 避免白白打一条包
                    if id != 0 {
                        let packet = UpdateBlockPacket::single(x, y, z, id, meta).encode();
                        out.push(reply(addr, vec![packet]));
                    }
                    return;
                }
                let block_id = id;
                // 门连带另一半 两格都要让所有人看到变空气与碎屑
                let mut cells = vec![(x, y, z, id, meta)];
                if let Some(other) = paired {
                    cells.push(other);
                }
                for &(bx, by, bz, _, _) in &cells {
                    out.extend(self.broadcast_block(bx, by, bz, 0, 0));
                }
                for &(bx, by, bz, bid, bmeta) in &cells {
                    out.extend(self.broadcast_destroy(bx, by, bz, bid, bmeta));
                }
                // 容器方块被挖掉 容器内容与打开关联都要一起清
                // 否则残留状态还会被 tick 甚至往空气里写方块
                if matches!(block_id, 61 | 62 | 58 | 84 | 86 | 116 | 117 | 118) {
                    self.containers.blocks.remove(&(x, y, z));
                    let viewers: Vec<std::net::SocketAddr> = self
                        .containers
                        .opened
                        .iter()
                        .filter(|(_, pos)| **pos == (x, y, z))
                        .map(|(a, _)| *a)
                        .collect();
                    for viewer in viewers {
                        self.containers.close(viewer);
                    }
                }
                // 掉落物交给方块属主分片生成 创造模式没有掉落
                // 按 Nukkit Block::getDrops 换算 有些方块状态位不能带进物品 damage
                if self.gamemode & GAMEMODE_CREATIVE == 0 {
                    for drop in crate::item::drops::drops_for(block_id, meta, x, y, z) {
                        let eid = self.next_eid;
                        self.next_eid += 1;
                        self.level.add_drop(x, z, eid, drop, (x, y, z));
                    }
                }
            }
            // 液体流动/重力方块下落等服务端自发改动 只广播给所有人 不碰背包/掉落
            RegionEvent::BlockChanged { cells } => {
                for cell in cells {
                    out.extend(self.broadcast_block(cell.x, cell.y, cell.z, cell.id, cell.meta));
                }
            }
            RegionEvent::FallRequested { x, y, z, id, meta } => {
                // 重力方块失去支撑 global 这里分配 eid 再回投给属主分片生成实体
                let eid = self.next_eid;
                self.next_eid += 1;
                let creative = self.gamemode & GAMEMODE_CREATIVE != 0;
                self.level.add_falling_block(x, y, z, eid, id, meta, creative);
            }
            RegionEvent::FallingBlockDropped { x, y, z, id, meta, creative } => {
                // 原版 FallingBlock 落地失败时 spawnAtLocation: 变回掉落物
                // 创造模式不出掉落 与挖掘掉落逻辑一致
                if !creative {
                    for drop in crate::item::drops::drops_for(id, meta, x, y, z) {
                        let eid = self.next_eid;
                        self.next_eid += 1;
                        self.level.add_drop(x, z, eid, drop, (x, y, z));
                    }
                }
            }
            RegionEvent::LaunchProjectile { kind, position, direction, speed } => {
                // 投掷物实体由抛出点分片生成 global 这里分配全局唯一 eid
                let eid = self.next_eid;
                self.next_eid += 1;
                self.level.launch_projectile(eid, kind, position, direction, speed);
            }
            RegionEvent::DamagePlayer { addr, damage } => {
                // 下落方块砸中 扣血并下发 SetHealth 受击动画给附近所有玩家
                let victim = self.players.get_mut(&addr).map(|state| {
                    state.health = (state.health - damage).max(0);
                    (state.health, state.eid, state.position)
                });
                if let Some((health, eid, position)) = victim {
                    out.push(reply(addr, vec![SetHealthPacket { health }.encode()]));
                    let hurt = EntityEventPacket { eid, event: ENTITY_EVENT_HURT }.encode();
                    out.extend(self.broadcast_at(
                        position.0.floor() as i32,
                        position.1.floor() as i32,
                        position.2.floor() as i32,
                        vec![hurt],
                        None,
                    ));
                }
            }
            RegionEvent::Pickup { addr, item } => {
                // 分片已经把掉落物移除了 这里把物品加进背包并刷新
                let updated = if let Some(state) = self.players.get_mut(&addr) {
                    state.add_item(item);
                    out.push(reply(addr, vec![state.inventory_packet().encode()]));
                    Some(state.clone())
                } else {
                    None
                };
                if let Some(state) = updated {
                    self.sync_player(addr, state);
                }
            }
            RegionEvent::DropsSnapshot { region, drops } => {
                self.drops_by_region.insert(region, drops);
                self.drops = self.drops_by_region.values().flatten().cloned().collect();
            }
            RegionEvent::PlayerRegion { addr, region, state } => {
                // 分片接管完了 归属表和位置镜像一起更新
                self.player_index.insert(addr, region);
                self.players.insert(addr, *state);
            }
            RegionEvent::Health { health } => {
                // 只存快照 分片退出后由 RegionIdle 顺手清掉
                self.region_health
                    .insert(RegionId { x: health.region.0, z: health.region.1 }, health);
            }
            RegionEvent::RegionIdle { region } => {
                // 还有玩家归属这里就不撤 分片退出后下次用到会按需重建
                if !self.player_index.values().any(|&id| id == region) {
                    if self.level.remove_region(region) {
                        // 分片没了 它的掉落物与健康快照也要一起清掉
                        self.region_health.remove(&region);
                        self.drops_by_region.remove(&region);
                        self.drops = self.drops_by_region.values().flatten().cloned().collect();
                    }
                }
            }
            RegionEvent::DoorToggled {
                addr: _,
                x,
                y,
                z,
                id,
                meta,
            } => {
                // 门真的翻了才到这 把新 meta 和开关音效一起推给视野内的人
                out.extend(self.broadcast_block(x, y, z, id, meta));
                let sound = LevelEventPacket::sound_door(x, y, z).encode();
                out.extend(self.broadcast_level_event(x, y, z, sound));
            }
            RegionEvent::ContainerOpen {
                addr,
                window_id: _,
                kind,
                slots: _,
                x,
                y,
                z,
                meta,
            } => {
                // 按坐标取已存的容器 关窗再开内容还在
                // 窗口 id 由注册表分配 同一个容器重复点击不再重发 open 只刷新内容
                let (state, already) = self.containers.open(addr, (x, y, z), kind, meta);
                let window_id = state.window_id;
                let mut bodies = Vec::new();
                if !already {
                    bodies.push(
                        ContainerOpenPacket {
                            window_id,
                            kind,
                            slots: state.slots.len() as u16,
                            x,
                            y,
                            z,
                            entity_id: -1,
                        }
                        .encode(),
                    );
                }
                bodies.push(
                    ContainerSetContentPacket {
                        window_id,
                        slots: state.slots.clone(),
                        hotbar: Vec::new(),
                    }
                    .encode(),
                );
                // 熔炉进度条 ContainerSetData 先不发 0.14 客户端收到会整进程闪退
                // 烧制逻辑照常跑 产物/燃料靠上面的 ContainerSetContent 与槽位更新同步
                let _ = kind;
                out.push(reply(addr, bodies));
            }
            RegionEvent::ContainerSlot {
                addr,
                window_id: _,
                slot,
                item,
            } => {
                self.containers.set_slot(addr, slot, item);
            }
            RegionEvent::ContainerClosed { addr } => {
                self.containers.close(addr);
            }
            RegionEvent::BlockPlaced {
                addr,
                origin,
                cells,
                item,
                ok,
            } => {
                if !ok {
                    // 目标格被占 没放下去 把扣掉的物品原样退回来
                    let updated = if self.gamemode & GAMEMODE_CREATIVE == 0 {
                        self.players.get_mut(&addr).and_then(|state| {
                            state.add_item(ItemStack::new(item, 1));
                            out.push(reply(addr, vec![state.inventory_packet().encode()]));
                            Some(state.clone())
                        })
                    } else {
                        None
                    };
                    if let Some(state) = updated {
                        self.sync_player(addr, state);
                    }
                    return;
                }
                // 门/床 一次两格 每格都要广播 客户端才会把另一半也画出来
                let mut written = vec![cells.first];
                if let Some(second) = cells.second {
                    written.push(second);
                }
                for cell in written {
                    out.extend(self.broadcast_block(
                        origin.0 + cell.dx,
                        origin.1 + cell.dy,
                        origin.2 + cell.dz,
                        cell.id,
                        cell.meta,
                    ));
                }
            }
        }
    }

    // global region 改过玩家状态后回写分片 保持权威与镜像一致
    fn sync_player(&mut self, addr: SocketAddr, state: crate::entity::PlayerState) {
        let Some(&id) = self.player_index.get(&addr) else {
            return;
        };
        self.level.sync_player(addr, id, state);
    }

}
