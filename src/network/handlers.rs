use std::net::SocketAddr;

use tracing::{info, warn};

use crate::entity::{PlayerData, RegionPlayer, assign_uuid};
use crate::level::actor::RegionEvent;
use crate::level::region::RegionId;
use crate::network::protocol::item::ItemStack;
use crate::network::protocol::login::LoginPacket;

use super::broadcast::reply;
use super::login::{login_sequence, protocol_accepted};
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

        let state = self.players.entry(addr).or_default();
        state.logged_in = true;
        state.eid = eid;
        state.uuid = uuid;
        state.username = login.username;
        state.skin = login.skin;
        state.position = spawn_position;
        if let Some(data) = saved {
            data.apply_to(state);
        }
        // 区块队列由 region 线程自己推进 这里只把视图交过去
        state.spawn_pending = accepted;
        let player_position = state.position;
        let snapshot = state.clone();
        // 登录日志带上分配到的 uuid 同一台设备重登应看到同一个值
        // 存档目录 playerdata/<uuid>.dat 就是按这个值找的
        info!(
            "[{addr}] {} login uuid {uuid} batch pids {:02x?}",
            state.username,
            bodies
                .iter()
                .filter_map(|body| body.first().copied())
                .collect::<Vec<_>>()
        );
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
            RegionEvent::BlockBroken { addr: _, x, y, z, id } => {
                let Some(block_id) = id else {
                    return;
                };
                out.extend(self.broadcast_block(x, y, z, 0, 0));
                // 掉落物交给方块属主分片生成 创造模式没有掉落
                if self.gamemode & GAMEMODE_CREATIVE == 0 {
                    let eid = self.next_eid;
                    self.next_eid += 1;
                    self.level.add_drop(
                        x,
                        z,
                        eid,
                        ItemStack::new(block_id as i16, 1),
                        (x, y, z),
                    );
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
            RegionEvent::RegionIdle { region } => {
                // 还有玩家归属这里就不撤 分片退出后下次用到会按需重建
                if !self.player_index.values().any(|&id| id == region) {
                    if self.level.remove_region(region) {
                        // 分片没了 它的掉落物镜像也要一起清掉
                        self.drops_by_region.remove(&region);
                        self.drops = self.drops_by_region.values().flatten().cloned().collect();
                    }
                }
            }
            RegionEvent::BlockPlaced {
                addr,
                x,
                y,
                z,
                id,
                ok,
            } => {
                if !ok {
                    // 目标格被占 没放下去 把扣掉的方块退回来
                    let updated = if self.gamemode & GAMEMODE_CREATIVE == 0 {
                        self.players.get_mut(&addr).and_then(|state| {
                            state.add_item(ItemStack::new(id as i16, 1));
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
                out.extend(self.broadcast_block(x, y, z, id, 0));
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
