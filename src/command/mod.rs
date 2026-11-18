// 命令表与解析 语法照搬 Nukkit 0.14 (TimeCommand/SayCommand/Level 的时间常量)
// 权限系统暂未实现 所有在线玩家都可执行
// Nukkit 参考: command/defaults/*.java、level/Level.java、lang/eng/lang.ini

use bytes::Bytes;
use std::net::SocketAddr;

use crate::network::protocol::text::MAX_MESSAGE_LEN;

pub mod version;
pub(crate) mod stop;
pub(crate) mod help;
pub(crate) mod list;
pub(crate) mod say;
pub(crate) mod tell;
pub(crate) mod seed;
pub(crate) mod time;
pub(crate) mod clear;
pub(crate) mod teleport;
pub(crate) mod tps;
pub(crate) mod top;
pub(crate) mod gamemode;
pub(crate) mod difficulty;
pub(crate) mod op;

pub use version::CommandVersion;
pub(crate) use stop::CommandStop;
pub(crate) use help::CommandHelp;
pub(crate) use list::CommandList;
pub(crate) use say::CommandSay;
pub(crate) use tell::CommandTell;
pub(crate) use seed::CommandSeed;
pub(crate) use time::CommandTime;
pub(crate) use teleport::TeleportCommand;
pub(crate) use tps::CommandTps;
pub(crate) use top::TopCommand;
pub(crate) use gamemode::CommandGameMode;
pub(crate) use difficulty::CommandDifficulty;
pub(crate) use op::CommandOp;

// 主循环发给 RakNet 层的下行命令
// 只带游戏包包体 0x8e 前缀由 RakNet 层封装时补 避免多一次整包拷贝
// 定义放 command 层 让各命令模块能直接产出下行包 不反向依赖 network
#[derive(Debug)]
pub enum SendCmd {
    GamePackets { addr: SocketAddr, bodies: Vec<Bytes> },
    // 登录完成后放开该连接的下行 之前丢弃的是旧会话在途的残包
    SessionReady { addr: SocketAddr },
    // 打到服务端控制台的日志行 例如 "<alice> 你好" 玩家聊天的服务端镜像
    Console(String),
}

// 命令执行结果与发送者解耦 玩家回文本包 控制台打印到终端
// 照搬 Nukkit Command.broadcastCommandMessage 的语义 但权限广播暂缺
pub(crate) struct CommandOutput {
    pub(crate) messages: Vec<String>,
    // 时间变更等需要同步给全体玩家的下行包
    pub(crate) sync: Vec<SendCmd>,
}

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
    Help(CommandHelp),
    List(CommandList),
    Say(CommandSay),
    Tell(CommandTell),
    Seed(CommandSeed),
    Time(CommandTime),
    Teleport(TeleportCommand),
    Tps(CommandTps),
    Top(TopCommand),
    GameMode(CommandGameMode),
    Difficulty(CommandDifficulty),
    Op(CommandOp),
    // 参数不对时里面是用法提示 未知命令时是命令名
    Version(CommandVersion),
    Stop(CommandStop),
    Unknown(String),
}

// 命令表: 名字 描述 层级候选 参数规则 解析入口的唯一数据源
// parse / complete / help / 控制台高亮都从这张表生成 加命令只改这一处
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Args {
    // 不接受参数 多出来的词标红
    None,
    // 下一词必须是 children 之一
    Subcommands,
    // 后面是自由文本或取值 按颜色表轮流上色
    Free,
}

// 权限等级 与 Nukkit 的 setPermission 对应 本版只做 OP/玩家两档
// 控制台始终是 OP 不受此限制
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Permission {
    // 所有玩家可执行
    Any,
    // 仅 OP 可执行
    Op,
}

pub struct CommandDef {
    pub name: &'static str,
    // 等价别名 与 name 共用同一解析入口 但不进 /help 主列表
    // 例如 gamemode 的 gm difficulty 的 diff version 的 ver
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub args: Args,
    pub children: &'static [CommandDef],
    // 该命令所需权限 子命令沿用父命令的即可
    pub permission: Permission,
    // 只有顶层命令需要解析入口 子命令为 None
    pub parse: Option<fn(&[&str]) -> Command>,
}

// 在表里按名字查一条命令 别名同样命中 表是 static 所以引用可以直接存进高亮状态机
pub fn find_command(defs: &'static [CommandDef], name: &str) -> Option<&'static CommandDef> {
    defs.iter()
        .find(|def| def.name == name || def.aliases.iter().any(|alias| *alias == name))
}

fn build_help(_args: &[&str]) -> Command {
    Command::Help(CommandHelp::Help)
}
fn build_list(_args: &[&str]) -> Command {
    Command::List(CommandList::List)
}
fn build_say(args: &[&str]) -> Command {
    parse_say(args)
}
fn build_tell(args: &[&str]) -> Command {
    parse_tell(args)
}
fn build_seed(_args: &[&str]) -> Command {
    Command::Seed(CommandSeed::Seed)
}
fn build_time(args: &[&str]) -> Command {
    parse_time(args)
}
fn build_tp(args: &[&str]) -> Command {
    Command::Teleport(TeleportCommand::parse(args))
}
fn build_tps(_args: &[&str]) -> Command {
    Command::Tps(CommandTps::Tps)
}
fn build_top(_args: &[&str]) -> Command {
    Command::Top(TopCommand::Top)
}
fn build_version(_args: &[&str]) -> Command {
    Command::Version(CommandVersion::Version)
}
fn build_stop(_args: &[&str]) -> Command {
    Command::Stop(CommandStop::Stop)
}
fn build_gamemode(args: &[&str]) -> Command {
    CommandGameMode::parse(args)
}
fn build_difficulty(args: &[&str]) -> Command {
    CommandDifficulty::parse(args)
}
fn build_op(args: &[&str]) -> Command {
    Command::Op(CommandOp::parse_set(args, true))
}
fn build_deop(args: &[&str]) -> Command {
    Command::Op(CommandOp::parse_set(args, false))
}
fn build_ops(_args: &[&str]) -> Command {
    Command::Op(CommandOp::List)
}

static NO_ARGS: &[CommandDef] = &[];
static TIME_VALUES: &[CommandDef] = &[
    CommandDef { name: "day", aliases: &[], description: "", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: None },
    CommandDef { name: "night", aliases: &[], description: "", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: None },
];
static TIME_SUBS: &[CommandDef] = &[
    CommandDef { name: "start", aliases: &[], description: "", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: None },
    CommandDef { name: "stop", aliases: &[], description: "", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: None },
    CommandDef { name: "query", aliases: &[], description: "", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: None },
    CommandDef { name: "set", aliases: &[], description: "", args: Args::Free, children: TIME_VALUES, permission: Permission::Any, parse: None },
    CommandDef { name: "add", aliases: &[], description: "", args: Args::Free, children: TIME_VALUES, permission: Permission::Any, parse: None },
];

// 别名与 name 共用 parse 解析入口 并出现在 Tab 补全里 但不进 /help 主列表
// 权限: 仅 help/say/tell 玩家可用 其余全部 OP 专属(控制台始终 OP)
pub static COMMANDS: &[CommandDef] = &[
    CommandDef { name: "help", aliases: &[], description: "Shows the help menu", args: Args::None, children: NO_ARGS, permission: Permission::Any, parse: Some(build_help) },
    CommandDef { name: "say", aliases: &[], description: "Broadcasts the given message as the sender", args: Args::Free, children: NO_ARGS, permission: Permission::Any, parse: Some(build_say) },
    CommandDef { name: "tell", aliases: &[], description: "A Mojang provided command.", args: Args::Free, children: NO_ARGS, permission: Permission::Any, parse: Some(build_tell) },
    CommandDef { name: "list", aliases: &[], description: "A Mojang provided command.", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_list) },
    CommandDef { name: "stop", aliases: &[], description: "A Mojang provided command.", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_stop) },
    CommandDef { name: "seed", aliases: &[], description: "A Mojang provided command.", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_seed) },
    CommandDef { name: "time", aliases: &[], description: "A Mojang provided command.", args: Args::Subcommands, children: TIME_SUBS, permission: Permission::Op, parse: Some(build_time) },
    CommandDef { name: "tp", aliases: &[], description: "Teleports the given player (or yourself) to another player or coordinates", args: Args::Free, children: NO_ARGS, permission: Permission::Op, parse: Some(build_tp) },
    CommandDef { name: "tps", aliases: &[], description: "Shows the server TPS and MSPT", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_tps) },
    CommandDef { name: "top", aliases: &[], description: "Shows CPU, memory and tick statistics", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_top) },
    CommandDef { name: "difficulty", aliases: &["diff"], description: "Sets the difficulty of the world", args: Args::Free, children: NO_ARGS, permission: Permission::Op, parse: Some(build_difficulty) },
    CommandDef { name: "gamemode", aliases: &["gm"], description: "Changes the player's game mode", args: Args::Free, children: NO_ARGS, permission: Permission::Op, parse: Some(build_gamemode) },
    CommandDef { name: "op", aliases: &[], description: "Adds a player to the operator list", args: Args::Free, children: NO_ARGS, permission: Permission::Op, parse: Some(build_op) },
    CommandDef { name: "deop", aliases: &[], description: "Removes a player from the operator list", args: Args::Free, children: NO_ARGS, permission: Permission::Op, parse: Some(build_deop) },
    CommandDef { name: "ops", aliases: &[], description: "Lists the operators", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_ops) },
    CommandDef { name: "version", aliases: &["ver"], description: "Gets the version of this server including any plugins in use", args: Args::None, children: NO_ARGS, permission: Permission::Op, parse: Some(build_version) },
];

// 给 parse 出的命令查它所需权限 供网络层统一鉴权
// 未知命令按 Any 处理(照常回 Unknown 提示 不额外报无权限)
pub fn permission_of(cmd: &Command) -> Permission {
    match cmd {
        Command::Help(_) | Command::Say(_) | Command::Tell(_) | Command::Unknown(_) => Permission::Any,
        _ => Permission::Op,
    }
}

// 转发到 help 模块 保留公共 API
pub fn help_lines() -> Vec<String> {
    CommandHelp::Help.lines()
}

// 输入以 / 开头 归一化后按空格切分 顶层命令一律查表
pub fn parse(input: &str) -> Option<Command> {
    let input = input.trim().strip_prefix('/')?;
    let mut args = input.split_whitespace();
    let name = args.next()?;
    let rest: Vec<&str> = args.collect();
    let Some(build) = find_command(COMMANDS, name).and_then(|def| def.parse) else {
        return Some(Command::Unknown(name.to_string()));
    };
    Some(build(&rest))
}

// 照搬 TimeCommand: start/stop/query 无参数 set/add 需参数
// set day -> TIME_DAY set night -> TIME_NIGHT 其他按非负整数解析(Math.max(0, ...))
// add 同样解析非负整数 setTime(getTime() + value) 不取模
fn parse_time(args: &[&str]) -> Command {
    if args.is_empty() {
        return Command::Unknown(TIME_USAGE.to_string());
    }
    match args[0] {
        "start" => Command::Time(CommandTime::Time(TimeOp::Start)),
        "stop" => Command::Time(CommandTime::Time(TimeOp::Stop)),
        "query" => Command::Time(CommandTime::Time(TimeOp::Query)),
        "set" if args.len() >= 2 => {
            let value = match args[1] {
                "day" => TimeValue::Day,
                "night" => TimeValue::Night,
                other => match other.parse::<i32>() {
                    Ok(value) => TimeValue::Ticks(value.max(0)),
                    Err(_) => return Command::Unknown(TIME_USAGE.to_string()),
                },
            };
            Command::Time(CommandTime::Time(TimeOp::Set(value)))
        }
        "add" if args.len() >= 2 => match args[1].parse::<i32>() {
            Ok(value) => Command::Time(CommandTime::Time(TimeOp::Add(value.max(0)))),
            Err(_) => Command::Unknown(TIME_USAGE.to_string()),
        },
        _ => Command::Unknown(TIME_USAGE.to_string()),
    }
}

// 照搬 SayCommand: 无参数回用法 参数用空格拼回一条消息
// 截断到聊天单条上限 免得超长文本撑爆 TextPacket 的 u16 字符串长度
fn parse_say(args: &[&str]) -> Command {
    if args.is_empty() {
        return Command::Say(CommandSay::Usage);
    }
    let message: String = args.join(" ").chars().take(MAX_MESSAGE_LEN).collect();
    Command::Say(CommandSay::Say(message))
}

// 照搬 TellCommand: 第一词是目标玩家名 其余拼回私聊内容 缺参数回用法
fn parse_tell(args: &[&str]) -> Command {
    if args.len() < 2 {
        return Command::Tell(CommandTell::Usage);
    }
    let target = args[0].to_string();
    let message: String = args[1..].join(" ").chars().take(MAX_MESSAGE_LEN).collect();
    Command::Tell(CommandTell::Tell { target, message })
}


// 终端 Tab 自动补全: 给定当前输入(可含前导 /) 返回候选命令/子命令
// 仅在末尾 token 上做前缀匹配 空格结尾时补全下一个位置的词
pub fn complete(input: &str) -> Vec<String> {
    // 末尾空格要在 trim 之前判断 trim 会把尾随空格去掉
    let trailing_space = input.ends_with(' ') || input.ends_with('\t') || input.is_empty();
    let trimmed = input.trim().trim_start_matches('/');
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if trailing_space {
        return candidates_for(&tokens, "");
    }
    let Some((prefix, before)) = tokens.split_last() else {
        return candidates_for(&[], "");
    };
    let candidates = candidates_for(before, prefix);
    // 唯一候选且已经打完整时返回空 再按 Tab 只响铃 不会把 list 重新插一遍
    if candidates.len() == 1 && candidates[0] == *prefix {
        return Vec::new();
    }
    candidates
}

// 根据已完成的词(before)逐层下钻命令表 再按当前词前缀(prefix)过滤
fn candidates_for(before: &[&str], prefix: &str) -> Vec<String> {
    let mut defs: &'static [CommandDef] = COMMANDS;
    for token in before {
        let Some(def) = find_command(defs, token) else {
            return Vec::new();
        };
        defs = def.children;
    }
    // 主名与别名都作为候选 前缀短的(ver/gm/diff)排在长的(version/gamemode/difficulty)前
    let mut candidates: Vec<String> = defs
        .iter()
        .flat_map(|def| std::iter::once(def.name).chain(def.aliases.iter().copied()))
        .filter(|name| name.starts_with(prefix))
        .map(|name| name.to_string())
        .collect();
    // 按字典序排 列候选和 Tab 选择的顺序都稳定 前缀短的(ver)排在长的(version)前面
    candidates.sort();
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    // /help 的每行带 § 颜色码 比对前先剥掉
    fn plain(line: &str) -> String {
        let mut out = String::new();
        let mut chars = line.chars();
        while let Some(ch) = chars.next() {
            if ch == '§' {
                chars.next();
            } else {
                out.push(ch);
            }
        }
        out
    }

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
        assert_eq!(
            parse("/time set day"),
            Some(Command::Time(CommandTime::Time(TimeOp::Set(TimeValue::Day))))
        );
        assert_eq!(
            parse("/time set night"),
            Some(Command::Time(CommandTime::Time(TimeOp::Set(TimeValue::Night))))
        );
        assert_eq!(
            parse("/time set 1234"),
            Some(Command::Time(CommandTime::Time(TimeOp::Set(
                TimeValue::Ticks(1234)
            ))))
        );
    }

    #[test]
    fn negative_values_are_clamped_like_nukkit() {
        // Nukkit 用 Math.max(0, Integer.parseInt(...))
        assert_eq!(
            parse("/time set -5"),
            Some(Command::Time(CommandTime::Time(TimeOp::Set(TimeValue::Ticks(0)))))
        );
        assert_eq!(
            parse("/time add -9"),
            Some(Command::Time(CommandTime::Time(TimeOp::Add(0))))
        );
    }

    #[test]
    fn parses_lifecycle_and_query() {
        assert_eq!(
            parse("/time start"),
            Some(Command::Time(CommandTime::Time(TimeOp::Start)))
        );
        assert_eq!(
            parse("/time stop"),
            Some(Command::Time(CommandTime::Time(TimeOp::Stop)))
        );
        assert_eq!(
            parse("/time query"),
            Some(Command::Time(CommandTime::Time(TimeOp::Query)))
        );
        // ? 别名按用户要求移除 不再是命令
        assert_eq!(parse("/?"), Some(Command::Unknown("?".to_string())));
        assert_eq!(parse("/help"), Some(Command::Help(CommandHelp::Help)));
        assert_eq!(parse("/seed"), Some(Command::Seed(CommandSeed::Seed)));
        assert_eq!(parse("/stop"), Some(Command::Stop(CommandStop::Stop)));
        assert_eq!(
            parse("/stop extra"),
            Some(Command::Stop(CommandStop::Stop))
        );
    }

    #[test]
    fn bad_syntax_and_plain_chat() {
        assert!(matches!(parse("/time"), Some(Command::Unknown(_))));
        assert!(matches!(parse("/time wat"), Some(Command::Unknown(_))));
        assert_eq!(parse("hello"), None);
    }

    #[test]
    fn completes_top_level_commands() {
        assert_eq!(complete("ti"), vec!["time".to_string()]);
        assert_eq!(
            complete(""),
            vec![
                "deop".to_string(),
                "diff".to_string(),
                "difficulty".to_string(),
                "gamemode".to_string(),
                "gm".to_string(),
                "help".to_string(),
                "list".to_string(),
                "op".to_string(),
                "ops".to_string(),
                "say".to_string(),
                "seed".to_string(),
                "stop".to_string(),
                "tell".to_string(),
                "time".to_string(),
                "top".to_string(),
                "tp".to_string(),
                "tps".to_string(),
                "ver".to_string(),
                "version".to_string(),
            ]
        );
        // 别名一并补全 前缀短的排在长的命令名前
        assert_eq!(complete("v"), vec!["ver".to_string(), "version".to_string()]);
        assert_eq!(complete("g"), vec!["gamemode".to_string(), "gm".to_string()]);
        assert_eq!(
            complete("d"),
            vec!["deop".to_string(), "diff".to_string(), "difficulty".to_string()]
        );
        // 已经打完整的唯一候选不再返回 再按 Tab 只响铃不动行
        assert!(complete("list").is_empty());
        assert!(complete("/list").is_empty());
        assert!(complete("version").is_empty());
    }

    #[test]
    fn command_table_drives_parse_completion_and_help() {
        let mut names: Vec<String> = COMMANDS
            .iter()
            .flat_map(|def| std::iter::once(def.name).chain(def.aliases.iter().copied()))
            .map(|name| name.to_string())
            .collect();
        names.sort();
        assert_eq!(complete(""), names, "顶层候选来自命令表主名与别名");
        let time = find_command(COMMANDS, "time").unwrap();
        let mut subs: Vec<String> = time
            .children
            .iter()
            .map(|def| def.name.to_string())
            .collect();
        subs.sort();
        assert_eq!(complete("time "), subs, "子命令候选来自 children");
        let lines = help_lines();
        for def in COMMANDS {
            assert!(def.parse.is_some(), "{} 缺解析入口", def.name);
            assert!(
                lines
                    .iter()
                    .any(|line| plain(line).starts_with(&format!("/{}:", def.name))),
                "{} 不在 /help 列表里",
                def.name
            );
            // 别名可用可补全 但不进 /help 主列表
            for alias in def.aliases {
                assert!(
                    !lines
                        .iter()
                        .any(|line| plain(line).starts_with(&format!("/{alias}:"))),
                    "{alias} 别名不应出现在 /help 列表里"
                );
            }
        }
        assert!(!lines.iter().any(|line| line.starts_with("/?:")));
    }

    #[test]
    fn parses_say_like_nukkit() {
        assert_eq!(parse("/say"), Some(Command::Say(CommandSay::Usage)));
        assert_eq!(
            parse("/say hello world"),
            Some(Command::Say(CommandSay::Say("hello world".to_string())))
        );
        assert_eq!(complete("sa"), vec!["say".to_string()]);
        // 超长消息截断到聊天上限 不会撑爆 TextPacket 的 u16 字符串长度
        let long = "x".repeat(MAX_MESSAGE_LEN + 10);
        let input = format!("/say {long}");
        assert!(matches!(
            parse(&input),
            Some(Command::Say(CommandSay::Say(message)))
                if message.chars().count() == MAX_MESSAGE_LEN
        ));
    }

    #[test]
    fn parses_tell_like_nukkit() {
        assert_eq!(parse("/tell"), Some(Command::Tell(CommandTell::Usage)));
        assert_eq!(
            parse("/tell bob"),
            Some(Command::Tell(CommandTell::Usage))
        );
        assert_eq!(
            parse("/tell bob hi there"),
            Some(Command::Tell(CommandTell::Tell {
                target: "bob".to_string(),
                message: "hi there".to_string()
            }))
        );
        // 别名 msg / w 暂未接入 这里只验证主命令
        assert_eq!(complete("te"), vec!["tell".to_string()]);
    }

    #[test]
    fn parses_tp_like_nukkit() {
        assert_eq!(parse("/tp"), Some(Command::Teleport(TeleportCommand::Usage)));
        assert_eq!(
            parse("/tp bob"),
            Some(Command::Teleport(TeleportCommand::Player {
                target: None,
                destination: "bob".to_string()
            }))
        );
        assert_eq!(
            parse("/tp alice bob"),
            Some(Command::Teleport(TeleportCommand::Player {
                target: Some("alice".to_string()),
                destination: "bob".to_string()
            }))
        );
        assert_eq!(
            parse("/tp 1 5 1"),
            Some(Command::Teleport(TeleportCommand::Coords {
                target: None,
                position: (1, 5, 1),
                rotation: None
            }))
        );
        assert_eq!(
            parse("/tp bob 1 5 1"),
            Some(Command::Teleport(TeleportCommand::Coords {
                target: Some("bob".to_string()),
                position: (1, 5, 1),
                rotation: None
            }))
        );
        assert_eq!(
            parse("/tp 1 5 1 90 -30"),
            Some(Command::Teleport(TeleportCommand::Coords {
                target: None,
                position: (1, 5, 1),
                rotation: Some((90, -30))
            }))
        );
        assert_eq!(
            parse("/tp bob 1 5 1 90 -30"),
            Some(Command::Teleport(TeleportCommand::Coords {
                target: Some("bob".to_string()),
                position: (1, 5, 1),
                rotation: Some((90, -30))
            }))
        );
        // y 越界夹进世界高度 与 TeleportCommand 的 clamp 一致
        assert_eq!(
            parse("/tp 1 999 1"),
            Some(Command::Teleport(TeleportCommand::Coords {
                target: None,
                position: (1, 128, 1),
                rotation: None
            }))
        );
        // 坐标不是整数按用法提示处理
        assert_eq!(
            parse("/tp 1.5 5 1"),
            Some(Command::Teleport(TeleportCommand::Usage))
        );
        // tp 不是唯一候选了 tps 也是 tp 开头
        assert_eq!(complete("tp"), vec!["tp".to_string(), "tps".to_string()]);
        assert_eq!(
            complete("t"),
            vec![
                "tell".to_string(),
                "time".to_string(),
                "top".to_string(),
                "tp".to_string(),
                "tps".to_string()
            ]
        );
    }

    #[test]
    fn parses_tps() {
        assert_eq!(parse("/tps"), Some(Command::Tps(CommandTps::Tps)));
        // 不接受参数 多打的词直接忽略 与其它 Args::None 命令一致
        assert_eq!(parse("/tps now"), Some(Command::Tps(CommandTps::Tps)));
        assert!(complete("tps").is_empty());
    }

    #[test]
    fn parses_op_commands_and_permission_map() {
        assert_eq!(
            parse("/op alice"),
            Some(Command::Op(CommandOp::Set { name: "alice".into(), op: true }))
        );
        assert_eq!(
            parse("/deop alice"),
            Some(Command::Op(CommandOp::Set { name: "alice".into(), op: false }))
        );
        assert_eq!(parse("/ops"), Some(Command::Op(CommandOp::List)));
        assert_eq!(parse("/op"), Some(Command::Op(CommandOp::Usage("op"))));
        // 玩家可用: help/say/tell 与未知命令
        for input in ["/help", "/say hi", "/tell bob hi", "/nope"] {
            assert_eq!(permission_of(&parse(input).unwrap()), Permission::Any, "{input}");
        }
        // 其余全部 OP 专属
        for input in [
            "/list", "/stop", "/seed", "/time set day", "/tp 1 2 3", "/tps", "/top",
            "/gamemode 1", "/difficulty 2", "/op x", "/deop x", "/ops", "/version", "/ver",
        ] {
            assert_eq!(permission_of(&parse(input).unwrap()), Permission::Op, "{input}");
        }
    }

    #[test]
    fn parses_gamemode_and_difficulty() {
        // gamemode: 数字与别名都进同一枚举 非法值回用法
        assert_eq!(
            parse("/gamemode creative"),
            Some(Command::GameMode(CommandGameMode::Set { mode: 1 }))
        );
        assert_eq!(
            parse("/gamemode 0"),
            Some(Command::GameMode(CommandGameMode::Set { mode: 0 }))
        );
        assert_eq!(
            parse("/gamemode"),
            Some(Command::GameMode(CommandGameMode::Usage))
        );
        assert_eq!(
            parse("/gamemode 3"),
            Some(Command::GameMode(CommandGameMode::Usage))
        );
        // difficulty: 0-3 与别名
        assert_eq!(
            parse("/difficulty hard"),
            Some(Command::Difficulty(CommandDifficulty::Set { difficulty: 3 }))
        );
        assert_eq!(
            parse("/difficulty 2"),
            Some(Command::Difficulty(CommandDifficulty::Set { difficulty: 2 }))
        );
        assert_eq!(
            parse("/difficulty"),
            Some(Command::Difficulty(CommandDifficulty::Usage))
        );
        assert_eq!(
            parse("/difficulty 9"),
            Some(Command::Difficulty(CommandDifficulty::Usage))
        );
        // 别名 gm / diff 走同一解析入口 结果必须与主命令完全一致
        assert_eq!(parse("/gm creative"), parse("/gamemode creative"));
        assert_eq!(parse("/gm"), parse("/gamemode"));
        assert_eq!(parse("/gm 3"), parse("/gamemode 3"));
        assert_eq!(parse("/diff hard"), parse("/difficulty hard"));
        assert_eq!(parse("/diff"), parse("/difficulty"));
        assert_eq!(parse("/diff 9"), parse("/difficulty 9"));
    }

    #[test]
    fn aliases_resolve_and_complete() {
        // 三个别名都能解析 且与原命令产生同一条 Command
        assert_eq!(parse("/ver"), parse("/version"));
        assert_eq!(parse("/gm 1"), parse("/gamemode 1"));
        assert_eq!(parse("/diff 2"), parse("/difficulty 2"));
        // 控制台高亮查表同样命中别名
        assert!(find_command(COMMANDS, "gm").is_some());
        assert!(find_command(COMMANDS, "diff").is_some());
        assert!(find_command(COMMANDS, "ver").is_some());
        // 未知别名仍是 Unknown
        assert_eq!(parse("/gms"), Some(Command::Unknown("gms".to_string())));
        // 别名进补全 排序稳定
        assert_eq!(complete("v"), vec!["ver".to_string(), "version".to_string()]);
        assert_eq!(complete("g"), vec!["gamemode".to_string(), "gm".to_string()]);
        assert_eq!(
            complete("d"),
            vec!["deop".to_string(), "diff".to_string(), "difficulty".to_string()]
        );
    }

    #[test]
    fn completes_time_subcommands_and_values() {
        assert_eq!(
            complete("time s"),
            vec!["set".to_string(), "start".to_string(), "stop".to_string()]
        );
        assert_eq!(complete("time set "), vec!["day".to_string(), "night".to_string()]);
        assert_eq!(complete("time set d"), vec!["day".to_string()]);
        assert!(complete("time set day").is_empty());
        assert_eq!(complete("/ti"), vec!["time".to_string()]);
        // 已经打完整 再 Tab 不动行
        assert!(complete("/time").is_empty());
    }
}
