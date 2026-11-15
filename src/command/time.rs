// 时间命令 照搬 Nukkit 0.14 TimeCommand 的语法与 Level 的时间常量
// 权限系统暂未实现 所有在线玩家都可执行
// Nukkit 参考: command/defaults/TimeCommand.java、level/Level.java

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::entity::PlayerState;
use crate::level::WorldTime;

use crate::command::{CommandOutput, SendCmd, TimeOp};

#[derive(Eq, PartialEq, Debug)]
pub enum CommandTime {
    Time(TimeOp),
}

impl CommandTime {
    // 照搬 TimeCommand: start/stop/query 无参数 set/add 需参数
    // set day -> TIME_DAY set night -> TIME_NIGHT 其他按非负整数解析(Math.max(0, ...))
    // add 同样解析非负整数 setTime(getTime() + value) 不取模
    pub(crate) fn execute(
        &self,
        world_time: &mut WorldTime,
        players: &HashMap<SocketAddr, PlayerState>,
    ) -> CommandOutput {
        let op = match self {
            CommandTime::Time(op) => *op,
        };
        let message = match op {
            TimeOp::Start => {
                world_time.start_time();
                "Restarted the time".to_string()
            }
            TimeOp::Stop => {
                world_time.stop_time();
                "Stopped the time".to_string()
            }
            TimeOp::Query => format!("Time is {}", world_time.time()),
            TimeOp::Set(value) => {
                let value = value.resolve();
                world_time.set_time(value);
                format!("Set the time to {value}")
            }
            TimeOp::Add(value) => {
                let now = world_time.time();
                world_time.set_time(now + value);
                format!("Added {value} to the time")
            }
        };

        // set/add/start/stop 立即同步 照搬 setTime/startTime/stopTime 里的 sendTime
        let mut sync = Vec::new();
        if !matches!(op, TimeOp::Query) {
            let bodies = world_time.time_packets();
            for peer in players.keys() {
                sync.push(SendCmd::GamePackets {
                    addr: *peer,
                    bodies: bodies.clone(),
                });
            }
        }

        CommandOutput {
            messages: vec![message],
            sync,
        }
    }
}
