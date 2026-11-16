// 终端颜色工具 游戏内文案用 § 码 控制台用 ANSI 转义
// fg() 给调用方直接拼 0xRRGGBB; sections() 把整段 § 文案翻译成 ANSI

pub fn fg(rgb: u32) -> String {
    format!(
        "\u{1b}[38;2;{};{};{}m",
        (rgb >> 16) & 0xff,
        (rgb >> 8) & 0xff,
        rgb & 0xff
    )
}

pub const RESET: &str = "\u{1b}[0m";

// 收尾复位 等价于游戏里的 §r
// 手工拼 ANSI 时用它兜底 否则后面的文本(含控制台输入行)会继承上一个颜色
pub fn reset() -> &'static str {
    RESET
}

// 给一段文字上色并自动收尾 省得每次手写 fg()..+ RESET
pub fn paint(rgb: u32, text: &str) -> String {
    format!("{}{}{}", fg(rgb), text, RESET)
}
// 样式码 与游戏内 §l §o §n §m 一一对应
pub const BOLD: &str = "\u{1b}[1m";
pub const ITALIC: &str = "\u{1b}[3m";
pub const UNDERLINE: &str = "\u{1b}[4m";
pub const STRIKE: &str = "\u{1b}[9m";

// ANSI 16 色的十六进制值(xterm 默认调色板)
// 给不走 § 码、想直接 fg(ANSI_RED) 的地方用
pub const ANSI_BLACK: u32 = 0x000000;
pub const ANSI_DARK_RED: u32 = 0x800000;
pub const ANSI_DARK_GREEN: u32 = 0x008000;
pub const ANSI_OLIVE: u32 = 0x808000;
pub const ANSI_DARK_BLUE: u32 = 0x000080;
pub const ANSI_PURPLE: u32 = 0x800080;
pub const ANSI_TEAL: u32 = 0x008080;
pub const ANSI_SILVER: u32 = 0xC0C0C0;
pub const ANSI_GREY: u32 = 0x808080;
pub const ANSI_RED: u32 = 0xFF0000;
pub const ANSI_GREEN: u32 = 0x00FF00;
pub const ANSI_YELLOW: u32 = 0xFFFF00;
pub const ANSI_BLUE: u32 = 0x0000FF;
pub const ANSI_MAGENTA: u32 = 0xFF00FF;
pub const ANSI_CYAN: u32 = 0x00FFFF;
pub const ANSI_WHITE: u32 = 0xFFFFFF;

// Paper 1.12.2 的映射: terminalconsoleappender 1.1.1 的 MinecraftFormattingConverter
// 见 example/Paper-archive/Spigot-Server-Patches/0210-...patch 里的
// PatternLayout pattern="%highlightError{... %minecraftFormatting{%msg} ...}"
// 用的是终端 16 色号(30-37 / 90-97) 而不是 RGB 所以实际观感取决于终端调色板
const PALETTE: [&str; 16] = [
    "\u{1b}[30m", // §0 black
    "\u{1b}[34m", // §1 dark blue
    "\u{1b}[32m", // §2 dark green
    "\u{1b}[36m", // §3 dark aqua
    "\u{1b}[31m", // §4 dark red
    "\u{1b}[35m", // §5 dark purple
    "\u{1b}[33m", // §6 gold
    "\u{1b}[37m", // §7 gray
    "\u{1b}[90m", // §8 dark gray
    "\u{1b}[94m", // §9 blue
    "\u{1b}[92m", // §a green
    "\u{1b}[96m", // §b aqua
    "\u{1b}[91m", // §c red
    "\u{1b}[95m", // §d light purple
    "\u{1b}[93m", // §e yellow
    "\u{1b}[97m", // §f white
];

// 把 § 颜色码翻译成 ANSI 序列
// ansi=false 时只剥掉颜色码 用于重定向到文件/管道的场景
// 不认识的控制符原样保留 免得吃掉正常文字里的 §
pub fn sections(text: &str, ansi: bool) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    // 有没有真的上过色 上过才在结尾补 reset
    let mut painted = false;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '§' {
            out.push(ch);
            continue;
        }
        let Some(code) = chars.next() else {
            out.push('§');
            break;
        };
        match code {
            '0'..='9' | 'a'..='f' | 'A'..='F' => {
                if ansi {
                    // 走到这里 to_digit 一定是 Some
                    let index = code.to_digit(16).unwrap_or(0) as usize;
                    out.push_str(PALETTE[index]);
                    painted = true;
                }
            }
            'r' | 'R' => {
                if ansi {
                    out.push_str(RESET);
                    painted = false;
                }
            }
            // 样式码 和颜色码一样直到 reset 才失效
            'l' | 'L' => {
                if ansi {
                    out.push_str(BOLD);
                    painted = true;
                }
            }
            'o' | 'O' => {
                if ansi {
                    out.push_str(ITALIC);
                    painted = true;
                }
            }
            'n' | 'N' => {
                if ansi {
                    out.push_str(UNDERLINE);
                    painted = true;
                }
            }
            'm' | 'M' => {
                if ansi {
                    out.push_str(STRIKE);
                    painted = true;
                }
            }
            _ => {
                out.push('§');
                out.push(code);
            }
        }
    }
    // 字符串结束统一回初始色 调用方不用每条都写 §r
    // 否则下一行日志(含控制台输入行)会继承上一个颜色
    if ansi && painted {
        out.push_str(RESET);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_section_codes() {
        let ansi = sections("§aok §cno", true);
        // §a = ANSI 92 亮绿, §c = 91 亮红 与 Paper 1.12.2 一致
        assert!(ansi.starts_with("\u{1b}[92mok "));
        assert!(ansi.contains("\u{1b}[91mno"));
        assert!(!ansi.contains('§'));
        // 结尾自动 reset 下一行不会继承颜色
        assert!(ansi.ends_with("\u{1b}[0m"), "结尾没补 reset: {ansi:?}");
    }

    #[test]
    fn supports_bold_italic_and_friends() {
        assert_eq!(
            sections("§lbold§r", true),
            "\u{1b}[1mbold\u{1b}[0m"
        );
        assert_eq!(
            sections("§oitalic§r", true),
            "\u{1b}[3mitalic\u{1b}[0m"
        );
        // 样式和颜色可以叠加 结尾自动复位
        let mixed = sections("§l§cboth", true);
        assert!(mixed.starts_with("\u{1b}[1m\u{1b}[91mboth"));
        assert!(mixed.ends_with("\u{1b}[0m"));
        assert_eq!(sections("§nunder§mstrike", false), "understrike");
    }

    #[test]
    fn paint_wraps_with_reset() {
        let red = paint(0xFF0000, "boom");
        assert!(red.starts_with("\u{1b}[38;2;255;0;0mboom"));
        assert_eq!(reset(), RESET);
        assert!(red.ends_with(reset()));
    }

    #[test]
    fn plain_text_has_no_trailing_reset() {
        // 没上过色就不该多出转义序列
        assert_eq!(sections("plain", true), "plain");
        // §r 收尾后也不需要再补
        assert_eq!(
            sections("§ared§r", true),
            "\u{1b}[92mred\u{1b}[0m"
        );
    }

    #[test]
    fn strips_codes_for_files() {
        assert_eq!(sections("§aok §rplain", false), "ok plain");
    }

    #[test]
    fn keeps_unknown_section_chars() {
        // § 后面不是颜色码时原样保留 避免吞字
        assert_eq!(sections("100§ off", true), "100§ off");
        // 结尾孤立的 § 也不能吞
        assert_eq!(sections("tail§", true), "tail§");
    }
}
