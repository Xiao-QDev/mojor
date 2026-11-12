// 命令解析 照搬 Nukkit 0.14 TimeCommand 的语法与 Level 的时间常量
// 权限系统暂未实现 所有在线玩家都可执行
// Nukkit 参考: command/defaults/TimeCommand.java、level/Level.java

pub mod version;
pub use version::CommandVersion;

// 时间常量照搬 cn.nukkit.level.Level
pub const TIME_DAY: i32 = 0;
pub const TIME_SUNSET: i32 = 12000;
pub const TIME_NIGHT: i32 = 14000;
pub const TIME_SUNRISE: i32 = 23000;
pub const TIME_FULL: i32 = 24000;

pub const TIME_USAGE: &str = "/time <start|stop|query|set|add>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeValue {
    Day,
    Night,
    Ticks(i32),
}

impl TimeValue {
    pub fn resolve(self) -> i32 {
        match self {
            TimeValue::Day => TIME_DAY,
            TimeValue::Night => TIME_NIGHT,
            TimeValue::Ticks(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOp {
    Start,
    Stop,
    Query,
    Set(TimeValue),
    Add(i32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    List,
    Seed,
    Time(TimeOp),
    // 参数不对时里面是用法提示 未知命令时是命令名
    Version(CommandVersion),
    Unknown(String),
}

pub fn help_lines() -> Vec<&'static str> {
    vec![
        "/help - 显示可用命令",
        "/list - 列出在线玩家",
        "/seed - 显示世界种子",
        "/time start|stop|query",
        "/time set <day|night|数值>",
        "/time add <数值>",
        "/version - 显示服务端版本",
    ]
}

// 输入以 / 开头 归一化后按空格切分
pub fn parse(input: &str) -> Option<Command> {
    let input = input.trim().strip_prefix('/')?;
    let mut args = input.split_whitespace();
    let name = args.next()?;
    let rest: Vec<&str> = args.collect();
    Some(match name {
        "help" | "?" => Command::Help,
        "list" => Command::List,
        "seed" => Command::Seed,
        "time" => parse_time(&rest),
        "version" | "ver" => Command::Version(CommandVersion::Version),
        other => Command::Unknown(other.to_string()),
    })
}

// 照搬 TimeCommand: start/stop/query 无参数 set/add 需参数
// set day -> TIME_DAY set night -> TIME_NIGHT 其他按非负整数解析(Math.max(0, ...))
// add 同样解析非负整数 setTime(getTime() + value) 不取模
fn parse_time(args: &[&str]) -> Command {
    if args.is_empty() {
        return Command::Unknown(TIME_USAGE.to_string());
    }
    match args[0] {
        "start" => Command::Time(TimeOp::Start),
        "stop" => Command::Time(TimeOp::Stop),
        "query" => Command::Time(TimeOp::Query),
        "set" if args.len() >= 2 => {
            let value = match args[1] {
                "day" => TimeValue::Day,
                "night" => TimeValue::Night,
                other => match other.parse::<i32>() {
                    Ok(value) => TimeValue::Ticks(value.max(0)),
                    Err(_) => return Command::Unknown(TIME_USAGE.to_string()),
                },
            };
            Command::Time(TimeOp::Set(value))
        }
        "add" if args.len() >= 2 => match args[1].parse::<i32>() {
            Ok(value) => Command::Time(TimeOp::Add(value.max(0))),
            Err(_) => Command::Unknown(TIME_USAGE.to_string()),
        },
        _ => Command::Unknown(TIME_USAGE.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_constants_match_nukkit() {
        assert_eq!(TIME_DAY, 0);
        assert_eq!(TIME_SUNSET, 12000);
        assert_eq!(TIME_NIGHT, 14000);
        assert_eq!(TIME_SUNRISE, 23000);
        assert_eq!(TIME_FULL, 24000);
    }

    #[test]
    fn parses_time_set_keywords() {
        assert_eq!(parse("/time set day"), Some(Command::Time(TimeOp::Set(TimeValue::Day))));
        assert_eq!(
            parse("/time set night"),
            Some(Command::Time(TimeOp::Set(TimeValue::Night)))
        );
        assert_eq!(
            parse("/time set 1234"),
            Some(Command::Time(TimeOp::Set(TimeValue::Ticks(1234))))
        );
    }

    #[test]
    fn negative_values_are_clamped_like_nukkit() {
        // Nukkit 用 Math.max(0, Integer.parseInt(...))
        assert_eq!(
            parse("/time set -5"),
            Some(Command::Time(TimeOp::Set(TimeValue::Ticks(0))))
        );
        assert_eq!(parse("/time add -9"), Some(Command::Time(TimeOp::Add(0))));
    }

    #[test]
    fn parses_lifecycle_and_query() {
        assert_eq!(parse("/time start"), Some(Command::Time(TimeOp::Start)));
        assert_eq!(parse("/time stop"), Some(Command::Time(TimeOp::Stop)));
        assert_eq!(parse("/time query"), Some(Command::Time(TimeOp::Query)));
        assert_eq!(parse("/?"), Some(Command::Help));
        assert_eq!(parse("/seed"), Some(Command::Seed));
    }

    #[test]
    fn bad_syntax_and_plain_chat() {
        assert!(matches!(parse("/time"), Some(Command::Unknown(_))));
        assert!(matches!(parse("/time wat"), Some(Command::Unknown(_))));
        assert_eq!(parse("hello"), None);
    }
}
