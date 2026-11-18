// /gamemode 命令 照搬 Nukkit 0.14 GameModeCommand 的语法与模式别名
// 本核心的 gamemode 是全局单一 u8 位标志(bit0=创造 0.14 客户端只有生存/创造两档)
// 因此这里只解析 0/1 与 survival/creative 别名 由 network 层切换全服并广播
// Nukkit 参考: command/defaults/GameModeCommand.java、lang/eng/lang.ini

use crate::command::Command;

// 0.14 只有生存/创造 数值 2(冒险)/3(旁观)客户端包结构未实现 按 Nukkit 用法提示处理
pub const GAMEMODE_USAGE: &str = "Usage: /gamemode <survival|creative>";

// 生存对应的内部模式值(与 network::GAMEMODE_SURVIVAL 一致)
pub const MODE_SURVIVAL: u8 = 0;
// 创造对应的内部模式值(与 network::GAMEMODE_CREATIVE 一致)
pub const MODE_CREATIVE: u8 = 1;

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum CommandGameMode {
    // mode 是内部 u8 位标志值 0=生存 1=创造
    Set { mode: u8 },
    Usage,
}

impl CommandGameMode {
    // 照搬 GameModeCommand: 数字 0/1 与 survival/creative 及其首字母缩写
    // Nukkit 0.14 只识别 survival(0)/creative(1) 其它数值走 usage
    pub(crate) fn parse(args: &[&str]) -> Command {
        if args.is_empty() {
            return Command::GameMode(CommandGameMode::Usage);
        }
        let mode = match args[0] {
            "0" | "survival" | "s" => MODE_SURVIVAL,
            "1" | "creative" | "c" => MODE_CREATIVE,
            _ => return Command::GameMode(CommandGameMode::Usage),
        };
        Command::GameMode(CommandGameMode::Set { mode })
    }

    // 回执文案 与 Nukkit commands.gamemode.success.self 一致
    pub fn success_message(mode: u8) -> String {
        let name = if mode & MODE_CREATIVE != 0 {
            "Creative"
        } else {
            "Survival"
        };
        format!("Game mode is now {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_and_alias_modes() {
        for arg in ["0", "survival", "s"] {
            assert_eq!(
                CommandGameMode::parse(&[arg]),
                Command::GameMode(CommandGameMode::Set { mode: MODE_SURVIVAL })
            );
        }
        for arg in ["1", "creative", "c"] {
            assert_eq!(
                CommandGameMode::parse(&[arg]),
                Command::GameMode(CommandGameMode::Set { mode: MODE_CREATIVE })
            );
        }
    }

    #[test]
    fn bad_and_missing_mode_falls_back_to_usage() {
        assert_eq!(
            CommandGameMode::parse(&[]),
            Command::GameMode(CommandGameMode::Usage)
        );
        // 0.14 未实现的冒险/旁观模式按用法提示处理
        assert_eq!(
            CommandGameMode::parse(&["2"]),
            Command::GameMode(CommandGameMode::Usage)
        );
        assert_eq!(
            CommandGameMode::parse(&["spectator"]),
            Command::GameMode(CommandGameMode::Usage)
        );
    }

    #[test]
    fn success_message_names_match_nukkit() {
        assert_eq!(
            CommandGameMode::success_message(MODE_SURVIVAL),
            "Game mode is now Survival"
        );
        assert_eq!(
            CommandGameMode::success_message(MODE_CREATIVE),
            "Game mode is now Creative"
        );
    }
}
