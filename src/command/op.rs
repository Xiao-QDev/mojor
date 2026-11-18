// /op /deop /ops 照搬 Nukkit 的 OP 管理语义
// 按用户名(大小写不敏感)增删 OP 名单 名单持久化由 NetworkState 层的 OpList 负责
// 三个命令都仅 OP 可用 控制台始终视为 OP

#[derive(Eq, PartialEq, Debug)]
pub enum CommandOp {
    // 增删目标玩家 括号内是回执要显示的名字
    Set { name: String, op: bool },
    // 列出当前 OP 名单
    List,
    // 参数不全给用法
    Usage(&'static str),
}

impl CommandOp {
    pub fn parse_set(args: &[&str], op: bool) -> Self {
        match args.first() {
            Some(name) if !name.is_empty() => CommandOp::Set {
                name: (*name).to_string(),
                op,
            },
            _ => CommandOp::Usage(if op { "op" } else { "deop" }),
        }
    }

    pub fn usage(cmd: &str) -> String {
        format!("Usage: /{cmd} <player>")
    }

    // 增删回执 与 Nukkit OpCommand/DeopCommand 文案一致
    pub fn set_message(name: &str, op: bool, changed: bool) -> String {
        if op {
            if changed {
                format!("Made {name} an operator")
            } else {
                format!("{name} is already an operator")
            }
        } else if changed {
            format!("Took {name} out of the operator list")
        } else {
            format!("{name} is not an operator")
        }
    }

    // /ops 名单文案 与 Nukkit list ops 一致
    pub fn list_message(names: &[&str]) -> Vec<String> {
        if names.is_empty() {
            return vec!["There are no ops".to_string()];
        }
        vec![
            format!("There are {} ops:", names.len()),
            names.join(", "),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_set_with_usage_fallback() {
        assert_eq!(
            CommandOp::parse_set(&["Alice"], true),
            CommandOp::Set { name: "Alice".into(), op: true }
        );
        assert_eq!(
            CommandOp::parse_set(&[], false),
            CommandOp::Usage("deop")
        );
    }

    #[test]
    fn messages_match_nukkit() {
        assert_eq!(CommandOp::set_message("Bob", true, true), "Made Bob an operator");
        assert_eq!(
            CommandOp::set_message("Bob", false, true),
            "Took Bob out of the operator list"
        );
        assert_eq!(CommandOp::list_message(&[]), vec!["There are no ops".to_string()]);
        assert_eq!(
            CommandOp::list_message(&["alice", "bob"]),
            vec!["There are 2 ops:".to_string(), "alice, bob".to_string()]
        );
    }
}
