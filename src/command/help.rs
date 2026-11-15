const HELP_ENTRIES: &[(&str, &str)] = &[
    ("help", "Shows the help menu"),
    ("list", "A Mojang provided command."),
    ("say", "Broadcasts the given message as the sender"),
    ("tell", "A Mojang provided command."),
    ("stop", "A Mojang provided command."),
    ("seed", "A Mojang provided command."),
    ("time", "A Mojang provided command."),
    ("version", "Gets the version of this server including any plugins in use"),
];

#[derive(Eq, PartialEq, Debug)]
pub enum CommandHelp {
    Help,
}

impl CommandHelp {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            "--------- Help: Index ---------------------------".to_string(),
            "Use /help [n] to get page n of help.".to_string(),
        ];
        for (name, description) in HELP_ENTRIES {
            lines.push(format!("/{}: {}", name, description));
        }
        lines
    }

    // 执行逻辑归到本模块 返回每行一条消息
    pub fn execute(&self) -> Vec<String> {
        self.lines()
    }
}
