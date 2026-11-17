//! 控制台输入: 独立线程读 stdin 不阻塞 tokio
//! 照 Paper 的 SimpleTerminalConsole 来 只是把 JLine 换成 Rust 的对应物 rustyline
//! - 行编辑/历史/Tab 补全/命令高亮 全部交给 rustyline 的 Helper 实现
//! - 日志走 ExternalPrinter 等价 TerminalConsoleAppender 擦掉输入行 打印 再重画
//! 终端不是 TTY(管道/测试)时退回逐行读取 行为不变

use std::borrow::Cow;
use std::io::{self, IsTerminal, Write};
use std::sync::{Mutex, OnceLock};

use rustyline::completion::{Completer, Pair};
use rustyline::config::CompletionType;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Config, Context, Editor, ExternalPrinter, Helper};
use tokio::sync::mpsc::Sender;
use tracing_subscriber::fmt::writer::MakeWriter;

use crate::command::{self, Args, COMMANDS, CommandDef, find_command};

const PROMPT: &str = "> ";
// 与 Paper 的 .console_history 对齐 放工作目录
const HISTORY_FILE: &str = ".console_history";

// 把补全/高亮/校验打包成 rustyline 的 Helper 等价 Paper 那几处 JLine 配置
struct ViolinHelper;

type ConsoleEditor = Editor<ViolinHelper, DefaultHistory>;

// 日志通道: 行编辑在跑时由 rustyline 负责擦行/打印/重画 不在跑时它直接写 tty
static PRINTER: OnceLock<Mutex<Box<dyn ExternalPrinter + Send>>> = OnceLock::new();

// 进程退出时可能正卡在 read 上 用这份副本把终端恢复回来
#[cfg(not(windows))]
static ORIG_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);

pub fn run(tx: Sender<String>) {
    // 非终端(管道/重定向/测试)直接逐行读取 不做行编辑
    if !io::stdin().is_terminal() {
        run_line_mode(tx);
        return;
    }
    match build_editor() {
        Some(mut editor) => {
            save_termios();
            let _ = read_loop(&mut editor, tx);
            restore_terminal();
        }
        None => run_line_mode(tx),
    }
}

fn build_editor() -> Option<ConsoleEditor> {
    let config = Config::builder()
        // 照搬 Paper 1.12.2 的 JLine 3 默认补全: 第一次 Tab 补公共前缀并列出候选
        // 第二次 Tab 进菜单 (vendor/rustyline 里打了补丁 见 lib.rs 的 violin patch)
        .completion_type(CompletionType::List)
        // JLine LineReaderImpl.DEFAULT_LIST_MAX = 100
        // 候选数达到这个数 或列表行数超过屏幕剩余行数时 第一次 Tab 才会问
        // "Violin: do you wish to see all N possibilities (M lines)?"
        // Paper 有 160 个命令 (>100) 所以会问 这里设成 8 让顶格 Tab 也能问
        // 想完全照 Paper 就把这里改回 100 想每次补全都问就设成 1
        .completion_prompt_limit(8)
        // 历史由我们自己加 空行和粘贴的多行不入历史
        .auto_add_history(false)
        .max_history_size(500)
        .ok()?
        .history_ignore_space(true)
        // 保留 ISIG: Ctrl+C 仍触发 SIGINT 走原来的优雅停机提示
        .enable_signals(true)
        .build();
    let mut editor = ConsoleEditor::with_config(config).ok()?;
    // with_config 不挂 Helper 补全/高亮都会失效 必须显式设置
    editor.set_helper(Some(ViolinHelper));
    let _ = editor.load_history(HISTORY_FILE);
    // 输出不是终端时拿不到 printer 日志退回直写 stdout
    if let Ok(printer) = editor.create_external_printer() {
        let _ = PRINTER.set(Mutex::new(Box::new(printer)));
    }
    Some(editor)
}

fn read_loop(editor: &mut ConsoleEditor, tx: Sender<String>) -> Result<(), ReadlineError> {
    loop {
        match editor.readline(PROMPT) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(line);
                let _ = editor.append_history(HISTORY_FILE);
                // 终端把粘贴的多行当成一行 这里按 Paper 的做法拆成多条命令
                for part in line.lines() {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    if tx.blocking_send(part.to_string()).is_err() {
                        return Ok(());
                    }
                }
            }
            // 关掉 ISIG 的终端(或别处改了 termios)会走到这里 补一个信号
            Err(ReadlineError::Interrupted) => {
                #[cfg(not(windows))]
                unsafe {
                    libc::raise(libc::SIGINT);
                }
                return Ok(());
            }
            Err(ReadlineError::Eof) => return Ok(()),
            Err(err) => return Err(err),
        }
    }
}

// 老逻辑: 逐行读取 仅在非 TTY 时使用
fn run_line_mode(tx: Sender<String>) {
    use std::io::BufRead;
    for line in io::stdin().lock().lines() {
        match line {
            Ok(line) if !line.trim().is_empty() => {
                if tx.blocking_send(line).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
}

fn save_termios() {
    #[cfg(not(windows))]
    unsafe {
        let mut orig: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut orig) == 0
            && let Ok(mut slot) = ORIG_TERMIOS.lock()
        {
            *slot = Some(orig);
        }
    }
}

// 任意退出路径都要恢复终端 否则 shell 会留在无回显状态
pub fn restore_terminal() {
    #[cfg(not(windows))]
    {
        let Ok(mut slot) = ORIG_TERMIOS.lock() else {
            return;
        };
        if let Some(orig) = slot.take() {
            unsafe {
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &orig);
            }
        }
    }
    // 让 shell 提示符另起一行 不要粘在 "> " 后面
    let _ = io::stdout().lock().write_all(b"\r\n");
}

// main 里持有 任何退出路径都会恢复终端
pub struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

pub fn terminal_guard() -> TerminalGuard {
    TerminalGuard
}

// 补全: 只在光标所在的那个词上做 与 JLine 的 COMPLETE_IN_WORD 等价
impl Completer for ViolinHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let candidates = command::complete(&line[..pos]);
        if candidates.is_empty() {
            return Ok((pos, Vec::new()));
        }
        // 光标处词的起点(pos 一定是字符边界 按 char 往回走)
        let mut start = pos;
        while start > 0 {
            let ch = line[..start].chars().next_back().unwrap();
            if ch == ' ' || ch == '\t' {
                break;
            }
            start -= ch.len_utf8();
        }
        // 首词带 / 前缀时保留斜杠 只替换命令名
        if line[start..].starts_with('/') {
            start += 1;
        }
        let pairs = candidates
            .into_iter()
            .map(|name| Pair {
                replacement: name.clone(),
                display: name,
            })
            .collect();
        Ok((start, pairs))
    }
}

impl Hinter for ViolinHelper {
    type Hint = String;
}

impl Highlighter for ViolinHelper {
    // 照 Paper 的 BrigadierCommandHighlighter: 认出来的命令字面量保持默认色
    // 参数依次换颜色 认不出来的部分标红
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        if line.is_empty() {
            return Cow::Borrowed(line);
        }
        let mut walker = Walker::default();
        let mut out = String::with_capacity(line.len() + 32);
        let mut prev_end = 0;
        for (start, end) in token_spans(line) {
            out.push_str(&line[prev_end..start]);
            let color = walker.next_color(&line[start..end]);
            if color.is_empty() {
                out.push_str(&line[start..end]);
            } else {
                out.push_str(color);
                out.push_str(&line[start..end]);
                out.push_str(RESET);
            }
            prev_end = end;
        }
        out.push_str(&line[prev_end..]);
        Cow::Owned(out)
    }
}

impl Validator for ViolinHelper {}

impl Helper for ViolinHelper {}

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
// Paper 用 CYAN YELLOW GREEN MAGENTA BLUE 轮流给参数上色
const ARG_COLORS: [&str; 5] = ["\x1b[36m", "\x1b[33m", "\x1b[32m", "\x1b[35m", "\x1b[34m"];

// 高亮状态机直接拿命令表里的节点 不再另维护一份命令名清单
#[derive(Default)]
enum Walker {
    #[default]
    Root,
    // 刚匹配上一条命令 后续按它的 args 规则走
    In(&'static CommandDef),
    // 自由参数/取值 按颜色表轮流上色
    Free { args: usize },
    Bad,
}

impl Walker {
    fn after(def: &'static CommandDef) -> Self {
        match def.args {
            Args::Free => Walker::Free { args: 0 },
            Args::None | Args::Subcommands => Walker::In(def),
        }
    }

    fn next_color(&mut self, token: &str) -> &'static str {
        match self {
            Walker::Root => {
                let name = token.strip_prefix('/').unwrap_or(token);
                let Some(def) = find_command(COMMANDS, name) else {
                    *self = Walker::Bad;
                    return RED;
                };
                *self = Walker::after(def);
                ""
            }
            Walker::In(def) if def.args == Args::Subcommands => {
                let Some(child) = find_command(def.children, token) else {
                    *self = Walker::Bad;
                    return RED;
                };
                *self = Walker::after(child);
                ""
            }
            Walker::Free { args } => {
                let color = ARG_COLORS[*args % ARG_COLORS.len()];
                *args += 1;
                color
            }
            // 无参命令后面还有词 或者已经走错 都标红
            Walker::In(_) | Walker::Bad => RED,
        }
    }
}

// 按空白切出每个词的字节区间
fn token_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, ch) in line.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                spans.push((s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}

// tracing 日志输出: 有行编辑时交给 rustyline 的 ExternalPrinter
// 它会擦掉输入行 打印日志 再把提示符和输入内容重画回来
pub struct ConsoleWriter;

impl<'a> MakeWriter<'a> for ConsoleWriter {
    type Writer = ConsoleLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ConsoleLogWriter { buf: Vec::new() }
    }
}

pub struct ConsoleLogWriter {
    buf: Vec<u8>,
}

impl Write for ConsoleLogWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.buf);
        // 行编辑还没起来(启动阶段/非 TTY)时原本怎样输出就怎样输出
        if let Some(printer) = PRINTER.get() {
            let text = String::from_utf8_lossy(&data).into_owned();
            let mut printer = printer.lock().unwrap_or_else(|e| e.into_inner());
            if printer.print(text).is_ok() {
                return Ok(());
            }
        }
        let mut out = io::stdout().lock();
        out.write_all(&data)?;
        out.flush()
    }
}

impl Drop for ConsoleLogWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_tokens_with_offsets() {
        assert_eq!(token_spans("time set day"), vec![(0, 4), (5, 8), (9, 12)]);
        assert_eq!(token_spans("  time  "), vec![(2, 6)]);
        assert!(token_spans("   ").is_empty());
    }

    #[test]
    fn highlights_literals_arguments_and_errors() {
        let helper = ViolinHelper;
        // 字面量不着色
        assert_eq!(helper.highlight("stop", 4), "stop");
        // 参数按颜色表上色
        let colored = helper.highlight("time set day", 12);
        assert!(colored.contains("\x1b[36mday\x1b[0m"));
        // 不认识的命令整行标红
        let bad = helper.highlight("nope", 4);
        assert!(bad.starts_with(RED) && bad.ends_with(RESET));
        // 多余参数标红
        let extra = helper.highlight("stop now", 8);
        assert!(extra.ends_with(&format!("{RED}now{RESET}")));
    }

    #[test]
    fn highlights_say_arguments() {
        let helper = ViolinHelper;
        // /say 是已注册命令 参数按颜色表轮流上色 不能整行标红
        let colored = helper.highlight("say hello world", 15);
        assert!(!colored.contains(RED), "{colored}");
        assert!(colored.contains("\x1b[36mhello\x1b[0m"));
        assert!(colored.contains("\x1b[33mworld\x1b[0m"));
        // ? 别名已从命令表移除 高亮也跟着标红
        assert!(helper.highlight("?", 1).starts_with(RED));
        assert_eq!(
            helper.complete("sa", 2, &Context::new(&DefaultHistory::new())).unwrap().1[0].replacement,
            "say"
        );
    }

    #[test]
    fn completes_word_at_cursor() {
        let helper = ViolinHelper;
        let history = DefaultHistory::new();
        let ctx = Context::new(&history);
        let (start, pairs) = helper.complete("time s", 6, &ctx).unwrap();
        assert_eq!(start, 5);
        assert_eq!(
            pairs.iter().map(|p| p.replacement.as_str()).collect::<Vec<_>>(),
            vec!["set", "start", "stop"]
        );
        // 候选顺序: v 补全出 ver 与 version 两个
        let (start, pairs) = helper.complete("v", 1, &ctx).unwrap();
        assert_eq!(start, 0);
        assert_eq!(
            pairs.iter().map(|p| p.replacement.as_str()).collect::<Vec<_>>(),
            vec!["ver", "version"]
        );
        // 带斜杠时保留斜杠 只替换命令名
        let (start, _) = helper.complete("/ti", 3, &ctx).unwrap();
        assert_eq!(start, 1);
        // 光标在行中 只补当前词
        let (start, pairs) = helper.complete("ti time", 2, &ctx).unwrap();
        assert_eq!(start, 0);
        assert_eq!(pairs[0].replacement, "time");
    }
}
