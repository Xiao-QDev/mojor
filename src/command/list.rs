// 列出在线玩家 照搬 Nukkit ListCommand 的语义
// 权限系统暂未实现 所有在线玩家都可执行

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::entity::PlayerState;

#[derive(Eq, PartialEq, Debug)]
pub enum CommandList {
    List,
}

impl CommandList {
    // 执行逻辑归到本模块 自行过滤已出生玩家并拼装文案
    // 与 Nukkit 一致 先发人数行 再发逗号分隔的名单
    pub fn execute(
        &self,
        players: &HashMap<SocketAddr, PlayerState>,
        max_players: u32,
    ) -> Vec<String> {
        let mut names: Vec<&str> = players
            .values()
            .filter(|state| state.spawned)
            .map(|state| state.username.as_str())
            .collect();
        names.sort_unstable();
        let mut messages = vec![format!(
            "There are {}/{} players online:",
            names.len(),
            max_players
        )];
        if !names.is_empty() {
            messages.push(names.join(", "));
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_server_reports_zero() {
        let output = CommandList::List.execute(&HashMap::new(), 20);
        assert_eq!(output, vec!["There are 0/20 players online:".to_string()]);
    }
}
