// /tp 照搬 Nukkit 0.14 TeleportCommand 的语法与 Player.teleport 的语义
// 权限系统暂未实现 所有在线玩家都可执行
// 参考: command/defaults/TeleportCommand.java、Player.java、lang/eng/lang.ini 的 commands.tp.*

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::entity::PlayerState;

// lang.ini: commands.tp.usage
pub const TP_USAGE: &str = "Usage: /tp [target player] <destination player> OR /tp [target player] <x> <y> <z> [<yaw> <pitch>]";

// lang.ini: commands.generic.ingame
pub const TP_INGAME: &str = "You can only perform this command as a player";

// vanilla CommandBase.getPlayer 的 PlayerNotFoundException 文案
// 用在"没有玩家实体当主体 又没有指定目标玩家"的场景
pub const CMD_TP: &str = "You must specify which player you wish to perform this action on.";

// 命令只算"谁去哪" 位置权威在 region 分片 真正的落地由网络层投递
#[derive(Eq, PartialEq, Debug)]
pub enum TeleportCommand {
    // 参数不对
    Usage,
    // 传到另一个玩家脚下 target 为 None 表示发送者自己
    Player {
        target: Option<String>,
        destination: String,
    },
    // 传到坐标 照搬 Nukkit 用整数解析 y 夹在 0..=128
    Coords {
        target: Option<String>,
        position: (i32, i32, i32),
        // 没给转向就沿用被传送者当前的朝向
        rotation: Option<(i32, i32)>,
    },
}

// 一次要落地的位移 网络层据此改镜像再投给属主分片
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TeleportMove {
    pub(crate) addr: SocketAddr,
    pub(crate) position: (f32, f32, f32),
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

pub(crate) struct TeleportOutcome {
    pub(crate) messages: Vec<String>,
    pub(crate) moves: Vec<TeleportMove>,
}

impl TeleportCommand {
    // 照搬 TeleportCommand 的参数个数分支 1/3 个参数是"自己去" 其余第一个词是目标玩家
    // 0.14 的 5 参数分支把第一个词当玩家查了(文档写的是自己加转向) 这里按文档实现
    pub(crate) fn parse(args: &[&str]) -> Self {
        match args.len() {
            0 => TeleportCommand::Usage,
            1 => TeleportCommand::Player {
                target: None,
                destination: args[0].to_string(),
            },
            2 => TeleportCommand::Player {
                target: Some(args[0].to_string()),
                destination: args[1].to_string(),
            },
            3 => match parse_coords(&args[..3]) {
                Some((position, _)) => TeleportCommand::Coords {
                    target: None,
                    position,
                    rotation: None,
                },
                None => TeleportCommand::Usage,
            },
            4 => match parse_coords(&args[1..4]) {
                Some((position, _)) => TeleportCommand::Coords {
                    target: Some(args[0].to_string()),
                    position,
                    rotation: None,
                },
                None => TeleportCommand::Usage,
            },
            5 => match parse_coords(&args[..5]) {
                Some((position, rotation)) => TeleportCommand::Coords {
                    target: None,
                    position,
                    rotation,
                },
                None => TeleportCommand::Usage,
            },
            6 => match parse_coords(&args[1..6]) {
                Some((position, rotation)) => TeleportCommand::Coords {
                    target: Some(args[0].to_string()),
                    position,
                    rotation,
                },
                None => TeleportCommand::Usage,
            },
            _ => TeleportCommand::Usage,
        }
    }

    pub(crate) fn resolve(
        &self,
        sender: Option<SocketAddr>,
        players: &HashMap<SocketAddr, PlayerState>,
    ) -> TeleportOutcome {
        match self {
            TeleportCommand::Usage => TeleportOutcome::message(TP_USAGE.to_string()),
            TeleportCommand::Player {
                target,
                destination,
            } => {
                let Some((_, destination_state)) = find(players, destination) else {
                    return TeleportOutcome::message(format!("Can't find player {destination}"));
                };
                let (origin_addr, origin_state) = match origin(target, sender, players) {
                    Ok(found) => found,
                    Err(line) => return TeleportOutcome::message(line),
                };
                // 站到对方脚下 朝向保持自己的 与 Entity.teleport(Position) 一致
                let line = format!(
                    "Teleported {} to {}",
                    origin_state.username, destination_state.username
                );
                TeleportOutcome {
                    messages: vec![line],
                    moves: vec![TeleportMove {
                        addr: origin_addr,
                        position: destination_state.position,
                        yaw: origin_state.yaw,
                        pitch: origin_state.pitch,
                    }],
                }
            }
            TeleportCommand::Coords {
                target,
                position,
                rotation,
            } => {
                let (origin_addr, origin_state) = match origin(target, sender, players) {
                    Ok(found) => found,
                    Err(line) => return TeleportOutcome::message(line),
                };
                let (x, y, z) = (
                    position.0 as f32,
                    position.1 as f32,
                    position.2 as f32,
                );
                let (yaw, pitch) = rotation
                    .map(|(yaw, pitch)| (yaw as f32, pitch as f32))
                    .unwrap_or((origin_state.yaw, origin_state.pitch));
                // 照搬 NukkitMath.round(x, 2) 的打印 整数坐标打成 1.0 这种形式
                let line = format!(
                    "Teleported {} to {x:.1}, {y:.1}, {z:.1}",
                    origin_state.username
                );
                TeleportOutcome {
                    messages: vec![line],
                    moves: vec![TeleportMove {
                        addr: origin_addr,
                        position: (x, y, z),
                        yaw,
                        pitch,
                    }],
                }
            }
        }
    }
}

impl TeleportOutcome {
    fn message(line: String) -> Self {
        Self {
            messages: vec![line],
            moves: Vec::new(),
        }
    }
}

// 坐标按整数解析 y 夹在 0..=128 与 TeleportCommand 的 Math.max/min 一致
fn parse_coords(args: &[&str]) -> Option<((i32, i32, i32), Option<(i32, i32)>)> {
    let x = args[0].parse::<i32>().ok()?;
    let y = args[1]
        .parse::<i32>()
        .ok()?
        .clamp(0, crate::level::format::chunk::CHUNK_HEIGHT as i32);
    let z = args[2].parse::<i32>().ok()?;
    let rotation = match (args.get(3), args.get(4)) {
        (Some(yaw), Some(pitch)) => Some((yaw.parse::<i32>().ok()?, pitch.parse::<i32>().ok()?)),
        _ => None,
    };
    Some(((x, y, z), rotation))
}

// 用户名大小写不敏感 只认已出生的玩家 与 ListCommand/TellCommand 一致
fn find<'a>(
    players: &'a HashMap<SocketAddr, PlayerState>,
    name: &str,
) -> Option<(SocketAddr, &'a PlayerState)> {
    players
        .iter()
        .find(|(_, state)| state.spawned && state.username.eq_ignore_ascii_case(name))
        .map(|(addr, state)| (*addr, state))
}

// target 为 None 时就是发送者自己 控制台没有实体分两种情形
// 完全没有连接(控制台)：vanilla CommandBase.getPlayer 抛 PlayerNotFoundException 即 CMD_TP
// 有连接但镜像里没有(未登录/已退出)：照搬 commands.generic.ingame
fn origin<'a>(
    target: &Option<String>,
    sender: Option<SocketAddr>,
    players: &'a HashMap<SocketAddr, PlayerState>,
) -> Result<(SocketAddr, &'a PlayerState), String> {
    let Some(name) = target else {
        let Some(addr) = sender else {
            return Err(CMD_TP.to_string());
        };
        return players
            .get(&addr)
            .map(|state| (addr, state))
            .ok_or_else(|| TP_INGAME.to_string());
    };
    find(players, name).ok_or_else(|| format!("Can't find player {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(name: &str, position: (f32, f32, f32)) -> PlayerState {
        let mut state = PlayerState::default();
        state.username = name.to_string();
        state.spawned = true;
        state.position = position;
        state.yaw = 45.0;
        state.pitch = 10.0;
        state
    }

    fn players() -> HashMap<SocketAddr, PlayerState> {
        let alice = "127.0.0.1:50001".parse().unwrap();
        let bob = "127.0.0.1:50002".parse().unwrap();
        HashMap::from([
            (alice, state("alice", (1.0, 5.0, 1.0))),
            (bob, state("bob", (20.0, 6.0, -3.0))),
        ])
    }

    // 一个参数是自己传过去 朝向不变
    #[test]
    fn resolves_player_destination() {
        let players = players();
        let sender = "127.0.0.1:50001".parse().unwrap();
        let command = TeleportCommand::Player {
            target: None,
            destination: "bob".to_string(),
        };
        let outcome = command.resolve(Some(sender), &players);
        assert_eq!(outcome.messages, vec!["Teleported alice to bob".to_string()]);
        assert_eq!(
            outcome.moves,
            vec![TeleportMove {
                addr: sender,
                position: (20.0, 6.0, -3.0),
                yaw: 45.0,
                pitch: 10.0,
            }]
        );
    }

    // 两参数是把 A 传到 B 大小写不敏感
    #[test]
    fn resolves_player_to_player() {
        let players = players();
        let command = TeleportCommand::Player {
            target: Some("ALICE".to_string()),
            destination: "bob".to_string(),
        };
        let outcome = command.resolve(None, &players);
        assert_eq!(outcome.moves.len(), 1);
        assert_eq!(outcome.moves[0].position, (20.0, 6.0, -3.0));
    }

    // 坐标版没给转向就沿用自己当前的
    #[test]
    fn resolves_coords_without_rotation() {
        let players = players();
        let sender = "127.0.0.1:50001".parse().unwrap();
        let command = TeleportCommand::Coords {
            target: None,
            position: (100, 20, 100),
            rotation: None,
        };
        let outcome = command.resolve(Some(sender), &players);
        assert_eq!(
            outcome.messages,
            vec!["Teleported alice to 100.0, 20.0, 100.0".to_string()]
        );
        assert_eq!(outcome.moves[0].yaw, 45.0);
        assert_eq!(outcome.moves[0].pitch, 10.0);
    }

    #[test]
    fn resolves_coords_with_rotation() {
        let players = players();
        let sender = "127.0.0.1:50001".parse().unwrap();
        let command = TeleportCommand::Coords {
            target: None,
            position: (1, 2, 3),
            rotation: Some((90, -30)),
        };
        let outcome = command.resolve(Some(sender), &players);
        assert_eq!(outcome.moves[0].yaw, 90.0);
        assert_eq!(outcome.moves[0].pitch, -30.0);
    }

    // 控制台不能"传自己" 目标不存在也要回可读的提示
    #[test]
    fn rejects_console_self_and_unknown_targets() {
        let players = players();
        let command = TeleportCommand::Player {
            target: None,
            destination: "bob".to_string(),
        };
        // 控制台没有玩家实体 vanilla 报 PlayerNotFoundException
        assert_eq!(command.resolve(None, &players).messages, vec![CMD_TP.to_string()]);

        // 连接在但镜像里没有这个玩家 才是 Nukkit 的 ingame 提示
        let ghost = "127.0.0.1:59999".parse().unwrap();
        assert_eq!(
            command.resolve(Some(ghost), &players).messages,
            vec![TP_INGAME.to_string()]
        );

        let command = TeleportCommand::Player {
            target: None,
            destination: "nobody".to_string(),
        };
        assert_eq!(
            command.resolve(Some("127.0.0.1:50001".parse().unwrap()), &players).messages,
            vec!["Can't find player nobody".to_string()]
        );
    }
}
