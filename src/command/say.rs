// /say 照搬 Nukkit 0.14 SayCommand 的语义 权限系统暂未实现
// 参考: command/defaults/SayCommand.java、lang/eng/lang.ini 的 chat.type.announcement=[{%0}] {%1}
// 控制台发送者显示为 "Server" 玩家发送者显示为用户名

use std::collections::HashMap;
use std::net::SocketAddr;

use bytes::Bytes;

use crate::command::{CommandOutput, SendCmd};
use crate::entity::PlayerState;
use crate::network::protocol::text::TextPacket;

// lang/eng/lang.ini: commands.say.usage=/say <message ...>
pub const SAY_USAGE: &str = "Usage: /say <message ...>";

#[derive(Eq, PartialEq, Debug)]
pub enum CommandSay {
    Say(String),
    Usage,
}

impl CommandSay {
    pub(crate) fn execute(
        &self,
        sender: &str,
        players: &HashMap<SocketAddr, PlayerState>,
    ) -> CommandOutput {
        let line = match self {
            CommandSay::Usage => {
                return CommandOutput {
                    messages: vec![SAY_USAGE.to_string()],
                    sync: Vec::new(),
                };
            }
            CommandSay::Say(message) => format!("[{sender}] {message}"),
        };
        let body: Bytes = TextPacket::raw(&line).encode();
        // broadcastMessage 发给全体在线玩家 包含命令发起者自己
        let sync = players
            .keys()
            .map(|peer| SendCmd::GamePackets {
                addr: *peer,
                bodies: vec![body.clone()],
            })
            .collect();
        CommandOutput {
            // 控制台不在 players 里 单独把广播文本打回终端
            messages: vec![line],
            sync,
        }
    }
}
