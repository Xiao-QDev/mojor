// /tell 照搬 Nukkit 0.14 TellCommand 的语义 权限系统暂未实现
// 参考: command/defaults/TellCommand.java、lang 的 commands.message.display.*
// 私聊只定向发给目标 不广播全服 发送者收到回执

use std::collections::HashMap;
use std::net::SocketAddr;

use bytes::Bytes;

use crate::command::{CommandOutput, SendCmd};
use crate::entity::PlayerState;
use crate::network::protocol::text::TextPacket;

pub const TELL_USAGE: &str = "Usage: /tell <player> <private message ...>";

#[derive(Eq, PartialEq, Debug)]
pub enum CommandTell {
    Usage,
    Tell { target: String, message: String },
}

impl CommandTell {
    pub(crate) fn execute(
        &self,
        sender: &str,
        players: &HashMap<SocketAddr, PlayerState>,
    ) -> CommandOutput {
        let (target, message) = match self {
            CommandTell::Usage => {
                return CommandOutput {
                    messages: vec![TELL_USAGE.to_string()],
                    sync: Vec::new(),
                };
            }
            CommandTell::Tell { target, message } => (target, message),
        };
        // 目标必须在线且已出生 用户名大小写不敏感匹配
        let Some((target_addr, target_state)) = players
            .iter()
            .find(|(_, state)| state.spawned && state.username.eq_ignore_ascii_case(target))
        else {
            return CommandOutput {
                messages: vec![format!("That player ({target}) is not online")],
                sync: Vec::new(),
            };
        };
        let target_name = target_state.username.clone();
        // 目标收到私聊: "<sender> whispers to you: <message>"
        let incoming = format!("{sender} whispers to you: {message}");
        let body: Bytes = TextPacket::raw(&incoming).encode();
        let sync = vec![SendCmd::GamePackets {
            addr: *target_addr,
            bodies: vec![body],
        }];
        // 发送者回执: "You whisper to <target>: <message>"
        let receipt = format!("You whisper to {target_name}: {message}");
        CommandOutput {
            messages: vec![receipt],
            sync,
        }
    }
}
