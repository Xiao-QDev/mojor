// /help 的单行格式: §f/§e<命令>§f: §f<描述>
// 斜杠与冒号是白色 命令名黄色 描述白色 与游戏内原版观感一致
const HELP_ENTRIES: &[(&str, &str)] = &[
    ("help", "shows the help menu"),
    ("list", "A Mojang provided command."),
    ("say", "Broadcasts the given message as the sender"),
    ("teleport", "A Mojang provided command."),
    ("tell", "A Mojang provided command."),
    ("time", "A Mojang provided command."),
    ("tp", "A Mojang provided command."),
    ("tps", "Gets the current ticks per second for the server."),
    ("top", "Real-time system metrics."),
    ("stop", "A Mojang provided command."),
    ("seed", "A Mojang provided command."),
    ("version", "Gets the version of this server including any plugins in use."),
];

#[derive(Eq, PartialEq, Debug)]
pub enum CommandHelp {
    Help,
}

impl CommandHelp {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            "§6--------- §f§lHelp: Index§r §6---------------------------".to_string(),
            "Use /help [n] to get page n of help.".to_string(),
        ];
        for (name, description) in HELP_ENTRIES {
            lines.push(format!("§6/{name}: §f§l{description}§r"));
        }
        lines
    }

    // 执行逻辑归到本模块 返回每行一条消息
    pub fn execute(&self) -> Vec<String> {
        self.lines()
    }
}
