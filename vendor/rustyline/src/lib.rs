//! Readline for Rust
//!
//! This implementation is based on [Antirez's
//! Linenoise](https://github.com/antirez/linenoise)
//!
//! # Example
//!
//! Usage
//!
//! ```
//! let mut rl = rustyline::DefaultEditor::new()?;
//! let readline = rl.readline(">> ");
//! match readline {
//!     Ok(line) => println!("Line: {:?}", line),
//!     Err(_) => println!("No input"),
//! }
//! # Ok::<(), rustyline::error::ReadlineError>(())
//! ```
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "custom-bindings")]
mod binding;
mod command;
pub mod completion;
pub mod config;
mod edit;
pub mod error;
pub mod highlight;
pub mod hint;
pub mod history;
mod keymap;
mod keys;
mod kill_ring;
mod layout;
pub mod line_buffer;
mod prompt;
#[cfg(feature = "with-sqlite-history")]
pub mod sqlite_history;
mod tty;
mod undo;
pub mod validate;

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::{fmt, result};

use log::debug;
#[cfg(feature = "derive")]
pub use rustyline_derive::{Completer, Helper, Highlighter, Hinter, Validator};

#[cfg(feature = "custom-bindings")]
pub use crate::binding::{ConditionalEventHandler, Event, EventContext, EventHandler};
use crate::completion::{Candidate, Completer, longest_common_prefix};
pub use crate::config::{Behavior, ColorMode, CompletionType, Config, EditMode, HistoryDuplicates};
use crate::edit::{RefreshKind, State};
use crate::error::ReadlineError;
use crate::highlight::{CmdKind, Highlighter};
use crate::hint::Hinter;
use crate::history::{DefaultHistory, History, SearchDirection};
pub use crate::keymap::{Anchor, At, CharSearch, Cmd, InputMode, Movement, RepeatCount, Word};
use crate::keymap::{Bindings, InputState, Refresher};
pub use crate::keys::{KeyCode, KeyEvent, Modifiers};
use crate::kill_ring::KillRing;
pub use crate::layout::GraphemeClusterMode;
use crate::layout::{Layout, Unit};
pub use crate::prompt::Prompt;
pub use crate::tty::ExternalPrinter;
use crate::tty::{Buffer, RawMode as _, RawReader as _, Renderer as _, Term, Terminal};
pub use crate::undo::Changeset;
use crate::validate::Validator;

/// The error type for I/O and Linux Syscalls (Errno)
pub type Result<T> = result::Result<T, ReadlineError>;

/// Completes the line/word
fn complete_line<H: Helper, P: Prompt + ?Sized>(
    rdr: &mut <Terminal as Term>::Reader,
    s: &mut State<'_, '_, H, P>,
    input_state: &mut InputState,
    config: &Config,
) -> Result<Option<Cmd>> {
    #[cfg(all(unix, feature = "with-fuzzy"))]
    use skim::prelude::{
        Skim, SkimItem, SkimItemReceiver, SkimItemSender, SkimOptionsBuilder, unbounded,
    };

    let completer = s.helper.unwrap();
    // get a list of completions
    let (start, candidates) = completer.complete(&s.line, s.line.pos(), &s.ctx)?;
    // if no completions, we are done
    if candidates.is_empty() {
        s.out.beep()?;
        Ok(None)
    } else if CompletionType::Circular == config.completion_type() {
        let mark = s.changes.begin();
        // Save the current edited line before overwriting it
        let backup = s.line.as_str().to_owned();
        let backup_pos = s.line.pos();
        let mut cmd;
        let mut i = 0;
        loop {
            // Show completion or original buffer
            if i < candidates.len() {
                let candidate = candidates[i].replacement();
                // TODO we can't highlight the line buffer directly
                /*let candidate = if let Some(highlighter) = s.highlighter {
                    highlighter.highlight_candidate(candidate, CompletionType::Circular)
                } else {
                    Borrowed(candidate)
                };*/
                completer.update(&mut s.line, start, candidate, &mut s.changes);
            } else {
                // Restore current edited line
                s.line.update(&backup, backup_pos, &mut s.changes);
            }
            s.refresh_line()?;

            cmd = s.next_cmd(input_state, rdr, true, true)?;
            match cmd {
                Cmd::Complete => {
                    i = (i + 1) % (candidates.len() + 1); // Circular
                    if i == candidates.len() {
                        s.out.beep()?;
                    }
                }
                Cmd::CompleteBackward => {
                    if i == 0 {
                        i = candidates.len(); // Circular
                        s.out.beep()?;
                    } else {
                        i = (i - 1) % (candidates.len() + 1); // Circular
                    }
                }
                Cmd::Abort => {
                    // Re-show original buffer
                    if i < candidates.len() {
                        s.line.update(&backup, backup_pos, &mut s.changes);
                        s.refresh_line()?;
                    }
                    s.changes.truncate(mark);
                    return Ok(None);
                }
                _ => {
                    s.changes.end();
                    break;
                }
            }
        }
        Ok(Some(cmd))
    } else if CompletionType::List == config.completion_type() {
        if let Some(lcp) = longest_common_prefix(&candidates) {
            // if we can extend the item, extend it
            if lcp.len() > s.line.pos() - start || candidates.len() == 1 {
                completer.update(&mut s.line, start, lcp, &mut s.changes);
                s.refresh_line()?;
            }
        }
        // beep if ambiguous
        if candidates.len() > 1 {
            s.out.beep()?;
        } else {
            return Ok(None);
        }
        // mojor patch: 照搬 Paper 1.12.2 用的 JLine 3 补全流程
        // doComplete 补公共前缀后 第一次 Tab 就走 doList(AUTO_LIST 默认开)
        // doList 里只有候选数 >= LIST_MAX(100) 或列表放不下屏幕时才问一句
        // 列表作为 post 画在输入行下面 输入行不动 再按 Tab 才进 doMenu
        let save_pos = s.line.pos();
        s.edit_move_end()?;
        s.line.set_pos(save_pos);
        let prefix = s.line.as_str()[start..].to_owned();
        let (num_rows, rows) = completion_menu_rows(s, &candidates, NO_SELECTION, &prefix);
        let prompt_lines = s.layout.end.row + 1;
        let list_max = config.completion_prompt_limit();
        let asked = (list_max > 0 && candidates.len() >= list_max)
            || num_rows >= s.out.get_rows().saturating_sub(prompt_lines);
        if asked {
            // getAppName() 在 Paper 里是 "Paper" 这里是 "Mojor"
            let msg = format!(
                "\nMojor: do you wish to see all {} possibilities ({} lines)?",
                candidates.len(),
                num_rows
            );
            s.out.write_and_flush(msg.as_str())?;
            // JLine 只读一个字符 只有 y/Y/Tab 继续 其它键取消
            let cmd = s.next_cmd(input_state, rdr, false, true)?;
            let yes = matches!(cmd, Cmd::SelfInsert(1, 'y' | 'Y') | Cmd::Complete);
            // 提示语留在屏幕上 输入行重画到它下面
            start_below_prompt(s)?;
            s.refresh_line()?;
            if !yes {
                // JLine 的 readCharacter 会吃掉回答用的字符 y/n 不会插进命令行
                // 回车等控制键交回主循环 免得按了没反应
                if matches!(cmd, Cmd::SelfInsert(..)) {
                    return Ok(None);
                }
                return Ok(Some(cmd));
            }
        }
        // 顶格 Tab 或者列表一屏放不下时 按 JLine 把列表打印成历史行再重画输入行
        if s.line.pos() == start || num_rows + 2 >= s.out.get_rows() {
            let _ = page_completions(rdr, s, input_state, &candidates)?;
            return Ok(None);
        }
        // 第一次 Tab: 候选列表画在输入行下面 没有选中项 只有前缀标青
        paint_menu_below(s, num_rows, Some(&rows))?;
        let cmd = s.next_cmd(input_state, rdr, true, true)?;
        if cmd != Cmd::Complete {
            paint_menu_below(s, num_rows, None)?;
            return Ok(Some(cmd));
        }
        {
            // mojor patch: 第二次 Tab 进 doMenu
            // 列表画在输入行下面 已输入的前缀标青 选中的一项反显
            // Tab 切换时上面的输入行跟着变 回车只确认选中项 Esc 取消
            let mut idx = candidates
                .iter()
                .position(|c| c.replacement() == prefix)
                .unwrap_or(0);
            let mark = s.changes.begin();
            let backup = s.line.as_str().to_owned();
            let backup_pos = s.line.pos();
            loop {
                completer.update(
                    &mut s.line,
                    start,
                    candidates[idx].replacement(),
                    &mut s.changes,
                );
                s.refresh_line()?;
                let (_, rows) = completion_menu_rows(s, &candidates, idx, &prefix);
                paint_menu_below(s, num_rows, Some(&rows))?;
                let cmd = s.next_cmd(input_state, rdr, true, true)?;
                match cmd {
                    Cmd::Complete => idx = (idx + 1) % candidates.len(),
                    Cmd::CompleteBackward => {
                        idx = if idx == 0 {
                            candidates.len() - 1
                        } else {
                            idx - 1
                        }
                    }
                    Cmd::Abort => {
                        paint_menu_below(s, num_rows, None)?;
                        s.line.update(&backup, backup_pos, &mut s.changes);
                        s.changes.truncate(mark);
                        s.refresh_line()?;
                        return Ok(None);
                    }
                    Cmd::AcceptLine | Cmd::Newline | Cmd::AcceptOrInsertLine { .. } => {
                        // mojor patch: 照搬 Paper 菜单 回车只是确认选中项 不提交
                        // 列表收起后命令行留在屏幕上 再按一次回车才执行
                        paint_menu_below(s, num_rows, None)?;
                        s.changes.end();
                        s.refresh_line()?;
                        return Ok(None);
                    }
                    _ => {
                        paint_menu_below(s, num_rows, None)?;
                        s.changes.end();
                        s.refresh_line()?;
                        return Ok(Some(cmd));
                    }
                }
            }
        }
    } else {
        // if fuzzy feature is enabled and on unix based systems check for the
        // corresponding completion_type
        #[cfg(all(unix, feature = "with-fuzzy"))]
        {
            use std::borrow::Cow;
            if CompletionType::Fuzzy == config.completion_type() {
                struct Candidate {
                    index: usize,
                    text: String,
                }
                impl SkimItem for Candidate {
                    fn text(&self) -> Cow<'_, str> {
                        Cow::Borrowed(&self.text)
                    }
                }

                let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();

                let _ = tx_item.send(
                    candidates
                        .iter()
                        .enumerate()
                        .map(|(i, c)| -> std::sync::Arc<dyn SkimItem> {
                            std::sync::Arc::new(Candidate {
                                index: i,
                                text: c.display().to_owned(),
                            })
                        })
                        .collect(),
                );
                drop(tx_item); // so that skim could know when to stop waiting for more items.

                // setup skim and run with input options
                // will display UI for fuzzy search and return selected results
                // by default skim multi select is off so only expect one
                // selection

                let options = SkimOptionsBuilder::default()
                    .prompt("? ")
                    .reverse(true)
                    .build()
                    .unwrap();

                let selected_items = Skim::run_with(options, Some(rx_item))
                    .map(|out| out.selected_items)
                    .unwrap_or_default();

                // match the first (and only) returned option with the candidate
                // and update the line otherwise only refresh
                // line to clear the skim UI changes
                if let Some(item) = selected_items.first() {
                    let item: &Candidate = (*item)
                        .as_any() // cast to Any
                        .downcast_ref::<Candidate>() // downcast to concrete type
                        .expect("something wrong with downcast");
                    if let Some(candidate) = candidates.get(item.index) {
                        completer.update(
                            &mut s.line,
                            start,
                            candidate.replacement(),
                            &mut s.changes,
                        );
                    }
                }
                s.refresh_line()?;
            }
        };
        Ok(None)
    }
}

/// Completes the current hint
fn complete_hint_line<H: Helper, P: Prompt + ?Sized>(s: &mut State<'_, '_, H, P>) -> Result<()> {
    let Some(hint) = s.hint.as_ref() else {
        return Ok(());
    };
    s.line.move_end();
    if let Some(text) = hint.completion() {
        if s.line.yank(text, 1, &mut s.changes).is_none() {
            s.out.beep()?;
        }
    } else {
        s.out.beep()?;
    }
    s.refresh_line()
}

fn page_completions<C: Candidate, H: Helper, P: Prompt + ?Sized>(
    rdr: &mut <Terminal as Term>::Reader,
    s: &mut State<'_, '_, H, P>,
    input_state: &mut InputState,
    candidates: &[C],
) -> Result<Option<Cmd>> {
    use std::cmp;

    let min_col_pad = 2;
    let cols = s.out.get_columns();
    let max_width = cmp::min(
        cols,
        candidates
            .iter()
            .map(|c| s.layout.width(c.display()))
            .max()
            .unwrap()
            + min_col_pad,
    );
    let num_cols = cols / max_width;
    let nbc = u16::try_from(candidates.len()).unwrap();

    let mut pause_row = s.out.get_rows() - 1;
    let num_rows = nbc.div_ceil(num_cols);
    let mut ab = String::new();
    for row in 0..num_rows {
        if row == pause_row {
            s.out.write_and_flush("\n--More--")?;
            let mut cmd = Cmd::Noop;
            while cmd != Cmd::SelfInsert(1, 'y')
                && cmd != Cmd::SelfInsert(1, 'Y')
                && cmd != Cmd::SelfInsert(1, 'n')
                && cmd != Cmd::SelfInsert(1, 'N')
                && cmd != Cmd::SelfInsert(1, 'q')
                && cmd != Cmd::SelfInsert(1, 'Q')
                && cmd != Cmd::SelfInsert(1, ' ')
                && cmd != Cmd::Kill(Movement::BackwardChar(1))
                && cmd != Cmd::AcceptLine
                && cmd != Cmd::Newline
                && !matches!(cmd, Cmd::AcceptOrInsertLine { .. })
            {
                cmd = s.next_cmd(input_state, rdr, false, true)?;
            }
            match cmd {
                Cmd::SelfInsert(1, 'y' | 'Y' | ' ') => {
                    pause_row += s.out.get_rows() - 1;
                }
                Cmd::AcceptLine | Cmd::Newline | Cmd::AcceptOrInsertLine { .. } => {
                    pause_row += 1;
                }
                _ => break,
            }
        }
        s.out.write_and_flush("\n")?;
        ab.clear();
        for col in 0..num_cols {
            let i = (col * num_rows) + row;
            if i < nbc {
                let candidate = &candidates[i as usize].display();
                let width = s.layout.width(candidate);
                if let Some(highlighter) = s.highlighter() {
                    ab.push_str(&highlighter.highlight_candidate(candidate, CompletionType::List));
                } else {
                    ab.push_str(candidate);
                }
                if ((col + 1) * num_rows) + row < nbc {
                    for _ in width..max_width {
                        ab.push(' ');
                    }
                }
            }
        }
        s.out.write_and_flush(ab.as_str())?;
    }
    s.out.write_and_flush("\n")?;
    s.repaint(RefreshKind::All)?;
    Ok(None)
}

// mojor patch: 候选菜单按列排版 已输入的前缀标蓝 选中的一项反显 和 Paper 的菜单一样
// JLine 的 doList 画候选列表时没有选中项 用 usize::MAX 表示
const NO_SELECTION: usize = usize::MAX;

fn completion_menu_rows<C: Candidate, H: Helper, P: Prompt + ?Sized>(
    s: &State<'_, '_, H, P>,
    candidates: &[C],
    selected: usize,
    prefix: &str,
) -> (u16, Vec<String>) {
    use std::cmp;

    // JLine toColumns 的列间距是 MARGIN_BETWEEN_COLUMNS = 3
    let min_col_pad = 3;
    let cols = s.out.get_columns();
    let max_width = cmp::min(
        cols,
        candidates
            .iter()
            .map(|c| s.layout.width(c.display()))
            .max()
            .unwrap()
            + min_col_pad,
    );
    let num_cols = cols / max_width;
    let nbc = u16::try_from(candidates.len()).unwrap();
    let num_rows = nbc.div_ceil(num_cols);
    let mut rows = Vec::with_capacity(usize::from(num_rows));
    for row in 0..num_rows {
        let mut ab = String::new();
        for col in 0..num_cols {
            let i = (col * num_rows) + row;
            if i < nbc {
                let i = usize::from(i);
                let candidate = candidates[i].display();
                let width = s.layout.width(candidate);
                let mut cell = if i == selected {
                    // 选中项只反显文字本身 白框长度跟着候选字数走
                    format!("\x1b[7m{candidate}\x1b[0m")
                } else if !prefix.is_empty() {
                    // JLine DEFAULT_COMPLETION_STYLE_STARTING = "36" 青色
                    // 只给已输入的那部分上色 剩下的正常色
                    match candidate.strip_prefix(prefix) {
                        Some(rest) => format!("\x1b[36m{prefix}\x1b[0m{rest}"),
                        None => candidate.to_string(),
                    }
                } else {
                    candidate.to_string()
                };
                // 列宽填充放在反显之外 保证白框不会拖到后面的空格上
                if ((col + 1) * num_rows) + row < nbc {
                    for _ in width..max_width {
                        cell.push(' ');
                    }
                }
                ab.push_str(&cell);
            }
        }
        rows.push(ab);
    }
    (num_rows, rows)
}

// mojor patch: 提示语自己占一行 换到下一行再把 layout 归零
// 这样下一次 refresh_line 会把输入行重画在提示语下面 而不是盖在提示语那一行
fn start_below_prompt<H: Helper, P: Prompt + ?Sized>(
    s: &mut State<'_, '_, H, P>,
) -> Result<()> {
    s.out.write_and_flush("\n")?;
    s.layout = Layout::new(s.layout.grapheme_cluster_mode);
    Ok(())
}

// mojor patch: 候选列表画在输入行下面 画完把光标放回输入行
// 屏幕上是 "> ver" 在上 候选列表在下 Paper 的菜单就是这样
fn paint_menu_below<H: Helper, P: Prompt + ?Sized>(
    s: &mut State<'_, '_, H, P>,
    num_rows: u16,
    rows: Option<&[String]>,
) -> Result<()> {
    use std::fmt::Write as _;

    let cursor = s.layout.cursor;
    let end_row = s.layout.end.row;
    let mut out = String::new();
    out.push('\r');
    // 先落到输入行块的最后一行 再换行进入列表区
    let down = end_row - cursor.row;
    if down > 0 {
        write!(out, "\x1b[{down}B").unwrap();
    }
    out.push('\n');
    for i in 0..num_rows {
        out.push_str("\r\x1b[K");
        if let Some(row) = rows.and_then(|rows| rows.get(usize::from(i))) {
            out.push_str(row);
        }
        if i + 1 < num_rows {
            out.push('\n');
        }
    }
    // 回到输入行块最后一行 再摆到 layout 记下的光标位置
    let up = end_row + num_rows - cursor.row;
    if up > 0 {
        write!(out, "\x1b[{up}A").unwrap();
    }
    out.push('\r');
    if cursor.col > 0 {
        write!(out, "\x1b[{}C", cursor.col).unwrap();
    }
    s.out.write_and_flush(out.as_str())
}

/// Incremental search
fn reverse_incremental_search<H: Helper, I: History, P: Prompt + ?Sized>(
    rdr: &mut <Terminal as Term>::Reader,
    s: &mut State<'_, '_, H, P>,
    input_state: &mut InputState,
    history: &I,
) -> Result<Option<Cmd>> {
    if history.is_empty() {
        return Ok(None);
    }
    let mark = s.changes.begin();
    // Save the current edited line (and cursor position) before overwriting it
    let backup = s.line.as_str().to_owned();
    let backup_pos = s.line.pos();

    let mut search_buf = String::new();
    let mut history_idx = history.len() - 1;
    let mut direction = SearchDirection::Reverse;
    let mut success = true;

    let mut cmd;
    // Display the reverse-i-search prompt and process chars
    loop {
        let prompt = if success {
            format!("(reverse-i-search)`{search_buf}': ")
        } else {
            format!("(failed reverse-i-search)`{search_buf}': ")
        };
        s.refresh_prompt_and_line(&prompt)?;

        cmd = s.next_cmd(input_state, rdr, true, true)?;
        if let Cmd::SelfInsert(_, c) = cmd {
            search_buf.push(c);
        } else {
            match cmd {
                Cmd::Kill(Movement::BackwardChar(_)) => {
                    search_buf.pop();
                    continue;
                }
                Cmd::ReverseSearchHistory => {
                    direction = SearchDirection::Reverse;
                    if history_idx > 0 {
                        history_idx -= 1;
                    } else {
                        success = false;
                        continue;
                    }
                }
                Cmd::ForwardSearchHistory => {
                    direction = SearchDirection::Forward;
                    if history_idx < history.len() - 1 {
                        history_idx += 1;
                    } else {
                        success = false;
                        continue;
                    }
                }
                Cmd::Abort => {
                    // Restore current edited line (before search)
                    s.line.update(&backup, backup_pos, &mut s.changes);
                    s.refresh_line()?;
                    s.changes.truncate(mark);
                    return Ok(None);
                }
                Cmd::Move(_) => {
                    s.refresh_line()?; // restore prompt
                    break;
                }
                _ => break,
            }
        }
        success = match history.search(&search_buf, history_idx, direction)? {
            Some(sr) => {
                history_idx = sr.idx;
                s.line.update(&sr.entry, sr.pos, &mut s.changes);
                true
            }
            _ => false,
        };
    }
    s.changes.end();
    Ok(Some(cmd))
}

struct Guard<'m>(&'m tty::Mode);

#[expect(unused_must_use)]
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        let Guard(mode) = *self;
        mode.disable_raw_mode();
    }
}

// Helper to handle backspace characters in a direct input
fn apply_backspace_direct(input: &str) -> String {
    // Setup the output buffer
    // No '\b' in the input in the common case, so set the capacity to the input
    // length
    let mut out = String::with_capacity(input.len());

    // Keep track of the size of each grapheme from the input
    // As many graphemes as input bytes in the common case
    let mut grapheme_sizes: Vec<u8> = Vec::with_capacity(input.len());

    for g in unicode_segmentation::UnicodeSegmentation::graphemes(input, true) {
        if g == "\u{0008}" {
            // backspace char
            if let Some(n) = grapheme_sizes.pop() {
                // Remove the last grapheme
                out.truncate(out.len() - n as usize);
            }
        } else {
            out.push_str(g);
            #[allow(clippy::cast_possible_truncation)]
            grapheme_sizes.push(g.len() as u8);
        }
    }

    out
}

fn readline_direct(
    mut reader: impl BufRead,
    mut writer: impl Write,
    validator: Option<&impl Validator>,
) -> Result<String> {
    let mut input = String::new();

    loop {
        if reader.read_line(&mut input)? == 0 {
            return Err(ReadlineError::Eof);
        }
        // Remove trailing newline
        let trailing_n = input.ends_with('\n');
        let trailing_r;

        if trailing_n {
            input.pop();
            trailing_r = input.ends_with('\r');
            if trailing_r {
                input.pop();
            }
        } else {
            trailing_r = false;
        }

        input = apply_backspace_direct(&input);

        match validator.as_ref() {
            None => return Ok(input),
            Some(v) => {
                let mut ctx = input.as_str();
                let mut ctx = validate::ValidationContext::new(&mut ctx);

                match v.validate(&mut ctx)? {
                    validate::ValidationResult::Valid(msg) => {
                        if let Some(msg) = msg {
                            writer.write_all(msg.as_bytes())?;
                        }
                        return Ok(input);
                    }
                    validate::ValidationResult::Invalid(Some(msg)) => {
                        writer.write_all(msg.as_bytes())?;
                    }
                    validate::ValidationResult::Incomplete => {
                        // Add newline and keep on taking input
                        if trailing_r {
                            input.push('\r');
                        }
                        if trailing_n {
                            input.push('\n');
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Syntax specific helper.
///
/// TODO Tokenizer/parser used for both completion, suggestion, highlighting.
/// (parse current line once)
pub trait Helper
where
    Self: Completer + Hinter + Highlighter + Validator,
{
}

impl Helper for () {}

/// Completion/suggestion context
pub struct Context<'h> {
    history: &'h dyn History,
    history_index: usize,
}

impl<'h> Context<'h> {
    /// Constructor. Visible for testing.
    #[must_use]
    pub fn new(history: &'h dyn History) -> Self {
        Self {
            history,
            history_index: history.len(),
        }
    }

    /// Return an immutable reference to the history object.
    #[must_use]
    pub fn history(&self) -> &dyn History {
        self.history
    }

    /// The history index we are currently editing
    #[must_use]
    pub fn history_index(&self) -> usize {
        self.history_index
    }
}

/// Line editor
#[must_use]
pub struct Editor<H: Helper, I: History> {
    term: Terminal,
    buffer: Option<Buffer>,
    history: I,
    helper: Option<H>,
    kill_ring: KillRing,
    config: Config,
    custom_bindings: Bindings,
}

/// Default editor with default helper and `DefaultHistory`
pub type DefaultEditor = Editor<(), DefaultHistory>;

impl<H: Helper> Editor<H, DefaultHistory> {
    /// Create an editor with the default configuration but no helper
    pub fn new() -> Result<Self> {
        Self::with_config(Config::default())
    }

    /// Create an editor with a specific configuration but no helper.
    pub fn with_config(config: Config) -> Result<Self> {
        let history = DefaultHistory::with_config(&config);
        Self::with_history(config, history)
    }

    /// Create an editor with a specific helper.
    pub fn with_helper(h: H) -> Result<Self> {
        let mut ed = Self::new();
        if let Ok(ref mut e) = ed {
            e.set_helper(Some(h));
        }
        ed
    }
}
impl<H: Helper + Default> Editor<H, DefaultHistory> {
    /// Create an editor with the default configuration and helper
    #[expect(clippy::should_implement_trait)]
    pub fn default() -> Result<Self> {
        Self::with_helper(H::default())
    }

    /// Create an editor with a specific configuration and default helper
    pub fn with_default(config: Config) -> Result<Self> {
        let mut ed = Self::with_config(config);
        if let Ok(ref mut e) = ed {
            e.set_helper(Some(H::default()));
        }
        ed
    }
}

impl<H: Helper, I: History> Editor<H, I> {
    /// Create an editor with a custom history impl.
    pub fn with_history(config: Config, history: I) -> Result<Self> {
        let term = Terminal::new(&config)?;
        Ok(Self {
            term,
            buffer: None,
            history,
            helper: None,
            kill_ring: KillRing::new(60),
            config,
            custom_bindings: Bindings::new(),
        })
    }

    /// This method will read a line from STDIN and will display a `prompt`.
    ///
    /// `prompt` should not be styled (in case the terminal doesn't support
    /// ANSI) directly: use [`Highlighter::highlight_prompt`] instead.
    ///
    /// It uses terminal-style interaction if `stdin` is connected to a
    /// terminal.
    /// Otherwise (e.g., if `stdin` is a pipe or the terminal is not supported),
    /// it uses file-style interaction.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn readline<P: Prompt + ?Sized>(&mut self, prompt: &P) -> Result<String> {
        self.readline_with(prompt, None)
    }

    /// This function behaves in the exact same manner as [`Editor::readline`],
    /// except that it pre-populates the input area.
    ///
    /// The text that resides in the input area is given as a 2-tuple.
    /// The string on the left of the tuple is what will appear to the left of
    /// the cursor and the string on the right is what will appear to the
    /// right of the cursor.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn readline_with_initial<P: Prompt + ?Sized>(
        &mut self,
        prompt: &P,
        initial: (&str, &str),
    ) -> Result<String> {
        self.readline_with(prompt, Some(initial))
    }

    fn readline_with<P: Prompt + ?Sized>(
        &mut self,
        prompt: &P,
        initial: Option<(&str, &str)>,
    ) -> Result<String> {
        if self.term.is_unsupported() {
            debug!(target: "rustyline", "unsupported terminal");
            // Write prompt and flush it to stdout
            let mut stdout = io::stdout();
            stdout.write_all(prompt.raw().as_bytes())?;
            stdout.flush()?;

            readline_direct(io::stdin().lock(), io::stderr(), self.helper.as_ref())
        } else if self.term.is_input_tty() {
            let (original_mode, term_key_map) = self.term.enable_raw_mode(&self.config)?;
            let guard = Guard(&original_mode);
            let user_input = self.readline_edit(prompt, initial, &original_mode, term_key_map);
            if self.config.auto_add_history()
                && let Ok(ref line) = user_input
            {
                self.add_history_entry(line.as_str())?;
            }
            drop(guard); // disable_raw_mode(original_mode)?;
            self.term.writeln()?;
            user_input
        } else {
            debug!(target: "rustyline", "stdin is not a tty");
            // Not a tty: read from file / pipe.
            readline_direct(io::stdin().lock(), io::stderr(), self.helper.as_ref())
        }
    }

    /// Handles reading and editing the readline buffer.
    /// It will also handle special inputs in an appropriate fashion
    /// (e.g., C-c will exit readline)
    fn readline_edit<P: Prompt + ?Sized>(
        &mut self,
        prompt: &P,
        initial: Option<(&str, &str)>,
        original_mode: &tty::Mode,
        term_key_map: tty::KeyMap,
    ) -> Result<String> {
        let mut stdout = self.term.create_writer(&self.config);

        self.kill_ring.reset(); // TODO recreate a new kill ring vs reset
        let ctx = Context::new(&self.history);
        let mut s = State::new(&mut stdout, prompt, self.helper.as_ref(), ctx);

        let mut input_state = InputState::new(&self.config, &self.custom_bindings);

        if let Some((left, right)) = initial {
            s.line.update(
                (left.to_owned() + right).as_ref(),
                left.len(),
                &mut s.changes,
            );
        }

        let mut rdr = self
            .term
            .create_reader(self.buffer.take(), &self.config, term_key_map)?;
        if self.term.is_output_tty()
            && self.config.check_cursor_position()
            && let Err(e) = s.move_cursor_at_leftmost(&mut rdr)
        {
            if let ReadlineError::Signal(error::Signal::Resize) = e {
                s.out.update_size();
            } else {
                return Err(e);
            }
        }
        s.refresh_line()?;

        loop {
            let mut cmd = s.next_cmd(&mut input_state, &mut rdr, false, false)?;

            if cmd.should_reset_kill_ring() {
                self.kill_ring.reset();
            }

            // First trigger commands that need extra input

            if cmd == Cmd::Complete && s.helper.is_some() {
                let next = complete_line(&mut rdr, &mut s, &mut input_state, &self.config)?;
                if let Some(next) = next {
                    cmd = next;
                } else {
                    continue;
                }
            }

            if cmd == Cmd::ReverseSearchHistory {
                // Search history backward
                let next =
                    reverse_incremental_search(&mut rdr, &mut s, &mut input_state, &self.history)?;
                if let Some(next) = next {
                    cmd = next;
                } else {
                    continue;
                }
            }

            #[cfg(unix)]
            if cmd == Cmd::Suspend {
                debug!(target: "rustyline", "SIGTSTP");
                original_mode.disable_raw_mode()?;
                tty::suspend()?;
                let _ = self.term.enable_raw_mode(&self.config)?; // TODO original_mode may have changed
                s.out.update_size(); // window may have been resized
                s.refresh_line()?;
                continue;
            }

            #[cfg(unix)]
            if cmd == Cmd::QuotedInsert {
                // Quoted insert
                let c = rdr.next_char()?;
                s.edit_insert(c, 1)?;
                continue;
            }

            #[cfg(windows)]
            if cmd == Cmd::PasteFromClipboard {
                let clipboard = rdr.read_pasted_text()?;
                s.edit_yank(&input_state, &clipboard[..], Anchor::Before, 1)?;
            }

            // Tiny test quirk
            #[cfg(test)]
            if matches!(
                cmd,
                Cmd::AcceptLine | Cmd::Newline | Cmd::AcceptOrInsertLine { .. }
            ) {
                self.term.cursor = s.layout.cursor.col as usize;
            }

            // Execute things can be done solely on a state object
            match command::execute(cmd, &mut s, &input_state, &mut self.kill_ring, &self.config)? {
                command::Status::Proceed => continue,
                command::Status::Submit => break,
            }
        }

        // Move to end, in case cursor was in the middle of the line, so that
        // next thing application prints goes after the input
        s.edit_move_buffer_end(CmdKind::ForcedRefresh)?;

        if cfg!(windows) {
            let _ = original_mode; // silent warning
        }
        self.buffer = rdr.unbuffer();
        Ok(s.line.into_string())
    }

    /// Load the history from the specified file.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn load_history<P: AsRef<Path> + ?Sized>(&mut self, path: &P) -> Result<()> {
        self.history.load(path.as_ref())
    }

    /// Save the history in the specified file.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn save_history<P: AsRef<Path> + ?Sized>(&mut self, path: &P) -> Result<()> {
        self.history.save(path.as_ref())
    }

    /// Append new entries in the specified file.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn append_history<P: AsRef<Path> + ?Sized>(&mut self, path: &P) -> Result<()> {
        self.history.append(path.as_ref())
    }

    /// Add a new entry in the history.
    ///
    /// # Errors
    /// Will return `Err` if entry cannot be persisted
    pub fn add_history_entry<S: AsRef<str> + Into<String>>(&mut self, line: S) -> Result<bool> {
        self.history.add(line.as_ref())
    }

    /// Clear history.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn clear_history(&mut self) -> Result<()> {
        self.history.clear()
    }

    /// Return a mutable reference to the history object.
    pub fn history_mut(&mut self) -> &mut I {
        &mut self.history
    }

    /// Return an immutable reference to the history object.
    pub fn history(&self) -> &I {
        &self.history
    }

    /// Register a callback function to be called for tab-completion
    /// or to show hints to the user at the right of the prompt.
    pub fn set_helper(&mut self, helper: Option<H>) {
        self.helper = helper;
    }

    /// Return a mutable reference to the helper.
    pub fn helper_mut(&mut self) -> Option<&mut H> {
        self.helper.as_mut()
    }

    /// Return an immutable reference to the helper.
    pub fn helper(&self) -> Option<&H> {
        self.helper.as_ref()
    }

    /// Bind a sequence to a command.
    #[cfg(feature = "custom-bindings")]
    pub fn bind_sequence<E: Into<Event>, R: Into<EventHandler>>(
        &mut self,
        key_seq: E,
        handler: R,
    ) -> Option<EventHandler> {
        self.custom_bindings
            .insert(Event::normalize(key_seq.into()), handler.into())
    }

    /// Remove a binding for the given sequence.
    #[cfg(feature = "custom-bindings")]
    pub fn unbind_sequence<E: Into<Event>>(&mut self, key_seq: E) -> Option<EventHandler> {
        self.custom_bindings
            .remove(&Event::normalize(key_seq.into()))
    }

    /// Returns an iterator over edited lines.
    /// Iterator ends at [EOF](ReadlineError::Eof).
    /// ```
    /// let mut rl = rustyline::DefaultEditor::new()?;
    /// for readline in rl.iter("> ") {
    ///     match readline {
    ///         Ok(line) => {
    ///             println!("Line: {}", line);
    ///         }
    ///         Err(err) => {
    ///             println!("Error: {:?}", err);
    ///             break;
    ///         }
    ///     }
    /// }
    /// # Ok::<(), rustyline::error::ReadlineError>(())
    /// ```
    pub fn iter<'a>(&'a mut self, prompt: &'a str) -> impl Iterator<Item = Result<String>> + 'a {
        Iter {
            editor: self,
            prompt,
        }
    }

    /// If output stream is a tty, this function returns its width and height as
    /// a number of characters.
    pub fn dimensions(&mut self) -> Option<(Unit, Unit)> {
        if self.term.is_output_tty() {
            let out = self.term.create_writer(&self.config);
            Some((out.get_columns(), out.get_rows()))
        } else {
            None
        }
    }

    /// Clear the screen.
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn clear_screen(&mut self) -> Result<()> {
        if self.term.is_output_tty() {
            let mut out = self.term.create_writer(&self.config);
            out.clear_screen()
        } else {
            Ok(())
        }
    }

    /// Create an external printer
    pub fn create_external_printer(&mut self) -> Result<<Terminal as Term>::ExternalPrinter> {
        self.term.create_external_printer()
    }

    /// Change cursor visibility
    ///
    /// # Errors
    /// Will return `Err` if an IO error occurs
    pub fn set_cursor_visibility(
        &mut self,
        visible: bool,
    ) -> Result<Option<<Terminal as Term>::CursorGuard>> {
        self.term.set_cursor_visibility(visible)
    }
}

impl<H: Helper, I: History> config::Configurer for Editor<H, I> {
    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    fn set_max_history_size(&mut self, max_size: usize) -> Result<()> {
        self.config_mut().set_max_history_size(max_size);
        self.history.set_max_len(max_size)
    }

    fn set_history_ignore_dups(&mut self, yes: bool) -> Result<()> {
        self.config_mut().set_history_ignore_dups(yes);
        self.history.ignore_dups(yes)
    }

    fn set_history_ignore_space(&mut self, yes: bool) {
        self.config_mut().set_history_ignore_space(yes);
        self.history.ignore_space(yes);
    }
}

impl<H: Helper, I: History> fmt::Debug for Editor<H, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Editor")
            .field("term", &self.term)
            .field("config", &self.config)
            .finish()
    }
}

struct Iter<'a, H: Helper, I: History> {
    editor: &'a mut Editor<H, I>,
    prompt: &'a str,
}

impl<H: Helper, I: History> Iterator for Iter<'_, H, I> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Result<String>> {
        let readline = self.editor.readline(self.prompt);
        match readline {
            Ok(l) => Some(Ok(l)),
            Err(ReadlineError::Eof) => None,
            e @ Err(_) => Some(e),
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(doctest)]
doc_comment::doctest!("../README.md");
