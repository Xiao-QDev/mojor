// /difficulty 命令 照搬 Nukkit 0.14 DifficultyCommand 的语法与难度别名
// 难度是全服属性 由 network 层更新状态并广播 SetDifficultyPacket
// Nukkit 参考: command/defaults/DifficultyCommand.java、level/Level.java

use crate::command::Command;
use crate::network::protocol::set_difficulty::{EASY, HARD, NORMAL, PEACEFUL};

pub const DIFFICULTY_USAGE: &str = "Usage: /difficulty <peaceful|easy|normal|hard>";

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum CommandDifficulty {
    // difficulty 是协议层难度值 0-3
    Set { difficulty: i32 },
    Usage,
}

impl CommandDifficulty {
    // 照搬 DifficultyCommand: 数字 0-3 与 peaceful/easy/normal/hard 及首字母缩写
    pub(crate) fn parse(args: &[&str]) -> Command {
        if args.is_empty() {
            return Command::Difficulty(CommandDifficulty::Usage);
        }
        let difficulty = match args[0] {
            "0" | "peaceful" | "p" => PEACEFUL,
            "1" | "easy" | "e" => EASY,
            "2" | "normal" | "n" => NORMAL,
            "3" | "hard" | "h" => HARD,
            _ => return Command::Difficulty(CommandDifficulty::Usage),
        };
        Command::Difficulty(CommandDifficulty::Set { difficulty })
    }

    // 回执文案 与 Nukkit commands.difficulty.success 一致
    pub fn success_message(difficulty: i32) -> String {
        let name = match difficulty {
            PEACEFUL => "Peaceful",
            EASY => "Easy",
            NORMAL => "Normal",
            HARD => "Hard",
            _ => "Normal",
        };
        format!("Difficulty is now {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_levels_by_number_and_alias() {
        for (num, aliases, value) in [
            ("0", ["p", "peaceful"], PEACEFUL),
            ("1", ["e", "easy"], EASY),
            ("2", ["n", "normal"], NORMAL),
            ("3", ["h", "hard"], HARD),
        ] {
            assert_eq!(
                CommandDifficulty::parse(&[num]),
                Command::Difficulty(CommandDifficulty::Set { difficulty: value })
            );
            for alias in aliases {
                assert_eq!(
                    CommandDifficulty::parse(&[alias]),
                    Command::Difficulty(CommandDifficulty::Set { difficulty: value })
                );
            }
        }
    }

    #[test]
    fn bad_and_missing_level_falls_back_to_usage() {
        assert_eq!(
            CommandDifficulty::parse(&[]),
            Command::Difficulty(CommandDifficulty::Usage)
        );
        assert_eq!(
            CommandDifficulty::parse(&["4"]),
            Command::Difficulty(CommandDifficulty::Usage)
        );
        assert_eq!(
            CommandDifficulty::parse(&["impossible"]),
            Command::Difficulty(CommandDifficulty::Usage)
        );
    }

    #[test]
    fn success_message_names_match_nukkit() {
        assert_eq!(
            CommandDifficulty::success_message(PEACEFUL),
            "Difficulty is now Peaceful"
        );
        assert_eq!(
            CommandDifficulty::success_message(HARD),
            "Difficulty is now Hard"
        );
    }
}
