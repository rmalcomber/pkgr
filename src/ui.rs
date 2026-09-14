//! A hand-rolled select prompt.
//!
//! Windows console handling is done directly against the Win32 API rather than
//! through a TUI crate. Key events come from `ReadConsoleInputW`, which reports
//! virtual key codes, so arrow keys arrive as events instead of ANSI escape
//! sequences that would have to be parsed back out of stdin. Drawing uses VT
//! sequences, which the console supports once
//! `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is switched on.

use std::io::{self, Write};

use crate::json::Task;

#[cfg(windows)]
use windows_sys::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, ReadConsoleInputW, SetConsoleMode,
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, INPUT_RECORD, KEY_EVENT, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE,
};

/// Most rows the list may occupy, so a project with many scripts scrolls in
/// place rather than flooding the scrollback.
const MAX_ROWS: usize = 15;

#[derive(Debug)]
pub enum Error {
    /// Esc or Ctrl+C.
    Cancelled,
    /// No console to draw on, e.g. stdin or stdout is redirected.
    NotInteractive,
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// What a keypress means to the picker.
#[derive(Debug, PartialEq, Clone, Copy)]
enum Key {
    Up,
    Down,
    Home,
    End,
    Enter,
    Cancel,
    Backspace,
    Char(char),
    Ignored,
}

/// The terminal operations the picker loop needs.
///
/// Splitting these out keeps the loop free of any Win32 call, so it can be
/// driven by a scripted console in tests. `Terminal` is the only production
/// implementation, and the generic is monomorphised into it, so this costs
/// nothing at runtime.
trait Console {
    fn width(&self) -> usize;
    fn write(&mut self, s: &str) -> io::Result<()>;
    fn clear(&mut self, lines: usize) -> io::Result<()>;
    fn read_key(&mut self) -> Result<Key, Error>;
}

/// Shows `tasks` and returns the index of the chosen one.
pub fn select(title: &str, tasks: &[Task]) -> Result<usize, Error> {
    if tasks.is_empty() {
        return Err(Error::Cancelled);
    }

    let mut term = Terminal::acquire()?;
    run_picker(&mut term, title, tasks)
}

/// The picker itself: draw a frame, read a key, repeat until the user commits
/// or cancels. Returns an index into `tasks`, not into the filtered view.
fn run_picker<C: Console>(term: &mut C, title: &str, tasks: &[Task]) -> Result<usize, Error> {
    // Indices of the tasks currently passing the filter.
    let mut shown: Vec<usize> = (0..tasks.len()).collect();
    let mut filter = String::new();
    let mut cursor = 0usize; // position within `shown`
    let mut scroll = 0usize; // first visible row
    let mut painted = 0usize; // lines drawn last frame, so they can be cleared

    let width = term.width();
    let pad = tasks
        .iter()
        .map(|t| t.name.chars().count())
        .max()
        .unwrap_or(0);

    loop {
        let rows = shown.len().min(MAX_ROWS);

        // Keep the cursor inside the visible window.
        if cursor < scroll {
            scroll = cursor;
        } else if rows > 0 && cursor >= scroll + rows {
            scroll = cursor + 1 - rows;
        }

        let view = View {
            title,
            filter: &filter,
            tasks,
            shown: &shown,
            cursor,
            scroll,
            rows,
            pad,
            width,
            painted,
        };
        let (frame, lines) = render(&view);

        term.write(&frame)?;
        painted = lines;

        match term.read_key()? {
            Key::Enter => {
                // An empty filter result has nothing under the cursor, so
                // Enter must do nothing rather than pick a stale index.
                if let Some(&idx) = shown.get(cursor) {
                    term.clear(painted)?;
                    return Ok(idx);
                }
            }
            Key::Cancel => {
                term.clear(painted)?;
                return Err(Error::Cancelled);
            }
            Key::Up => cursor = cursor.saturating_sub(1),
            Key::Down => {
                if cursor + 1 < shown.len() {
                    cursor += 1;
                }
            }
            Key::Home => cursor = 0,
            Key::End => cursor = shown.len().saturating_sub(1),
            Key::Backspace => {
                filter.pop();
                shown = matching(tasks, &filter);
                cursor = 0;
                scroll = 0;
            }
            Key::Char(c) => {
                filter.push(c);
                shown = matching(tasks, &filter);
                cursor = 0;
                scroll = 0;
            }
            Key::Ignored => {}
        }
    }
}

/// Everything one frame needs to draw itself.
struct View<'a> {
    title: &'a str,
    filter: &'a str,
    tasks: &'a [Task],
    shown: &'a [usize],
    cursor: usize,
    scroll: usize,
    rows: usize,
    pad: usize,
    width: usize,
    painted: usize,
}

/// Builds one frame and reports how many lines it occupies, so the next redraw
/// knows how far up to move before clearing.
fn render(v: &View<'_>) -> (String, usize) {
    let mut frame = String::new();

    if v.painted > 0 {
        // Return to the top of the previous frame and wipe it.
        frame.push_str(&format!("\x1b[{}A", v.painted));
        frame.push_str("\x1b[0J");
    }

    frame.push_str(&format!("\x1b[1m{}\x1b[0m", v.title));
    if !v.filter.is_empty() {
        frame.push_str(&format!("  \x1b[2mfilter: {}\x1b[0m", v.filter));
    }
    frame.push('\n');
    let mut lines = 1;

    if v.shown.is_empty() {
        frame.push_str("  \x1b[2m(nothing matches)\x1b[0m\n");
        lines += 1;
    }

    for row in 0..v.rows {
        let idx = v.shown[v.scroll + row];
        let text = label(&v.tasks[idx], v.pad, v.width);

        if v.scroll + row == v.cursor {
            // Reverse video reads correctly on any colour scheme.
            frame.push_str(&format!("\x1b[7m> {text}\x1b[0m\n"));
        } else {
            frame.push_str(&format!("  {text}\n"));
        }
        lines += 1;
    }

    let hidden = v.shown.len().saturating_sub(v.rows);
    if hidden > 0 {
        frame.push_str(&format!("  \x1b[2m... {hidden} more\x1b[0m\n"));
        lines += 1;
    }

    frame.push_str("\x1b[2m  up/down move · type to filter · enter run · esc cancel\x1b[0m\n");
    lines += 1;

    (frame, lines)
}

/// Case-insensitive substring match over the name and command, which is what
/// filtering a script list actually needs.
///
/// Case is folded for ASCII only. Script and task names are ASCII in practice,
/// and full Unicode folding would link std's case-mapping tables. Non-ASCII
/// characters still match, just case-sensitively.
fn matching(tasks: &[Task], filter: &str) -> Vec<usize> {
    (0..tasks.len())
        .filter(|&i| {
            contains_ignore_ascii_case(&tasks[i].name, filter)
                || contains_ignore_ascii_case(&tasks[i].command, filter)
        })
        .collect()
}

/// `haystack.contains(needle)`, ignoring ASCII case, without allocating.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    n.is_empty() || h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// Renders one row as "name    command", clipped to the terminal.
fn label(task: &Task, pad: usize, width: usize) -> String {
    let detail = if task.command.is_empty() {
        task.description.clone()
    } else if task.description.is_empty() {
        task.command.clone()
    } else {
        format!("{}  ({})", task.command, task.description)
    };

    let row = if detail.is_empty() {
        task.name.clone()
    } else {
        format!("{:<pad$}  {}", task.name, detail, pad = pad)
    };

    // Leave room for the "> " marker.
    let limit = width.saturating_sub(4);
    if limit > 1 && row.chars().count() > limit {
        let mut clipped: String = row.chars().take(limit - 1).collect();
        clipped.push('…');
        return clipped;
    }
    row
}

// ---------------------------------------------------------------------------
// Windows console
// ---------------------------------------------------------------------------

#[cfg(windows)]
struct Terminal {
    stdin: HANDLE,
    stdout: HANDLE,
    original_input_mode: u32,
    original_output_mode: u32,
    out: io::Stdout,
}

#[cfg(windows)]
impl Terminal {
    /// Switches the console into raw mode, or reports that there is no console.
    fn acquire() -> Result<Self, Error> {
        unsafe {
            let stdin = GetStdHandle(STD_INPUT_HANDLE);
            let stdout = GetStdHandle(STD_OUTPUT_HANDLE);

            let mut input_mode = 0u32;
            let mut output_mode = 0u32;

            // GetConsoleMode fails on a redirected handle, which doubles as the
            // interactivity check: no console mode means nothing to draw on.
            if GetConsoleMode(stdin, &mut input_mode) == 0
                || GetConsoleMode(stdout, &mut output_mode) == 0
            {
                return Err(Error::NotInteractive);
            }

            // Line input and echo would buffer keys until Enter and print them.
            // Dropping ENABLE_PROCESSED_INPUT delivers Ctrl+C as a key event so
            // it cancels the picker instead of killing the process.
            let raw =
                input_mode & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT);
            SetConsoleMode(stdin, raw);
            SetConsoleMode(stdout, output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);

            let mut term = Terminal {
                stdin,
                stdout,
                original_input_mode: input_mode,
                original_output_mode: output_mode,
                out: io::stdout(),
            };
            term.write("\x1b[?25l")?; // hide the cursor while drawing
            Ok(term)
        }
    }

    fn write_raw(&mut self, s: &str) -> io::Result<()> {
        self.out.write_all(s.as_bytes())?;
        self.out.flush()
    }
}

// Virtual key codes, which is why input is read as events rather than as ANSI
// escape sequences parsed back out of stdin.
#[cfg(windows)]
mod vk {
    pub const BACK: u16 = 0x08;
    pub const RETURN: u16 = 0x0D;
    pub const ESCAPE: u16 = 0x1B;
    pub const PRIOR: u16 = 0x21; // page up
    pub const NEXT: u16 = 0x22; // page down
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const UP: u16 = 0x26;
    pub const DOWN: u16 = 0x28;
}

/// Maps one key event to what it means to the picker.
///
/// Kept separate from the console read so the mapping can be tested without a
/// console: everything here is a pure function of the two values the kernel
/// reports.
#[cfg(windows)]
fn decode_key(virtual_key: u16, unicode_char: u16) -> Key {
    match virtual_key {
        vk::UP => Key::Up,
        vk::DOWN => Key::Down,
        vk::HOME | vk::PRIOR => Key::Home,
        vk::END | vk::NEXT => Key::End,
        vk::RETURN => Key::Enter,
        vk::ESCAPE => Key::Cancel,
        vk::BACK => Key::Backspace,
        _ => match char::from_u32(unicode_char as u32) {
            // Ctrl+C arrives as an ordinary control character now that
            // ENABLE_PROCESSED_INPUT is off.
            Some('\u{3}') => Key::Cancel,
            Some(c) if !c.is_control() => Key::Char(c),
            _ => Key::Ignored,
        },
    }
}

#[cfg(windows)]
impl Console for Terminal {
    fn width(&self) -> usize {
        unsafe {
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(self.stdout, &mut info) != 0 {
                let w = (info.srWindow.Right - info.srWindow.Left + 1) as usize;
                if w > 0 {
                    return w;
                }
            }
            80
        }
    }

    fn write(&mut self, s: &str) -> io::Result<()> {
        self.write_raw(s)
    }

    /// Erases the drawn frame so the picker leaves no trace behind.
    fn clear(&mut self, lines: usize) -> io::Result<()> {
        if lines > 0 {
            self.write_raw(&format!("\x1b[{lines}A\x1b[0J"))?;
        }
        Ok(())
    }

    fn read_key(&mut self) -> Result<Key, Error> {
        unsafe {
            loop {
                let mut record: INPUT_RECORD = std::mem::zeroed();
                let mut read = 0u32;

                if ReadConsoleInputW(self.stdin, &mut record, 1, &mut read) == 0 || read == 0 {
                    return Err(Error::Cancelled);
                }
                if record.EventType != KEY_EVENT as u16 {
                    continue;
                }

                let key = record.Event.KeyEvent;
                if key.bKeyDown == 0 {
                    continue; // key-up events would double every press
                }

                return Ok(decode_key(key.wVirtualKeyCode, key.uChar.UnicodeChar));
            }
        }
    }
}

#[cfg(windows)]
impl Drop for Terminal {
    /// Restores the console however the picker exits, including on a panic.
    fn drop(&mut self) {
        let _ = self.write("\x1b[?25h");
        unsafe {
            SetConsoleMode(self.stdin, self.original_input_mode);
            SetConsoleMode(self.stdout, self.original_output_mode);
        }
    }
}

// ---------------------------------------------------------------------------
// Placeholder for the eventual Linux port
// ---------------------------------------------------------------------------

#[cfg(not(windows))]
struct Terminal;

#[cfg(not(windows))]
impl Terminal {
    fn acquire() -> Result<Self, Error> {
        // A termios raw-mode implementation goes here when this is ported.
        Err(Error::NotInteractive)
    }
}

#[cfg(not(windows))]
impl Console for Terminal {
    fn width(&self) -> usize {
        80
    }
    fn write(&mut self, _s: &str) -> io::Result<()> {
        Ok(())
    }
    fn clear(&mut self, _lines: usize) -> io::Result<()> {
        Ok(())
    }
    fn read_key(&mut self) -> Result<Key, Error> {
        Err(Error::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(name: &str, command: &str) -> Task {
        Task {
            name: name.into(),
            command: command.into(),
            description: String::new(),
        }
    }

    fn tasks(names: &[&str]) -> Vec<Task> {
        names.iter().map(|n| task(n, "cmd")).collect()
    }

    /// A console driven by a fixed list of keypresses, capturing what the
    /// picker draws. Lets the whole loop be exercised without a real terminal.
    struct Scripted {
        keys: std::collections::VecDeque<Key>,
        frames: Vec<String>,
        cleared: Vec<usize>,
        width: usize,
    }

    impl Scripted {
        fn new(keys: &[Key]) -> Self {
            Scripted {
                keys: keys.iter().copied().collect(),
                frames: Vec::new(),
                cleared: Vec::new(),
                width: 200,
            }
        }

        /// The frame drawn just before the last key was consumed.
        fn last_frame(&self) -> &str {
            self.frames.last().map(String::as_str).unwrap_or("")
        }

        /// Names rendered in a frame, in the order they appear, with the
        /// selected one prefixed by ">".
        fn rows(frame: &str) -> Vec<String> {
            frame
                .lines()
                .filter(|l| l.contains("cmd"))
                .map(|l| {
                    let selected = l.contains("\x1b[7m");
                    let name = l
                        .replace("\x1b[7m", "")
                        .replace("\x1b[0m", "")
                        .trim_start_matches(['>', ' '])
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if selected {
                        format!(">{name}")
                    } else {
                        name
                    }
                })
                .collect()
        }
    }

    impl Console for Scripted {
        fn width(&self) -> usize {
            self.width
        }
        fn write(&mut self, s: &str) -> io::Result<()> {
            self.frames.push(s.to_string());
            Ok(())
        }
        fn clear(&mut self, lines: usize) -> io::Result<()> {
            self.cleared.push(lines);
            Ok(())
        }
        fn read_key(&mut self) -> Result<Key, Error> {
            // Running dry means the test forgot to commit or cancel; ending the
            // loop is better than hanging.
            self.keys.pop_front().ok_or(Error::Cancelled)
        }
    }

    /// Types each character as a filter keystroke.
    fn typed(s: &str) -> Vec<Key> {
        s.chars().map(Key::Char).collect()
    }

    fn press(keys: &[Key], names: &[&str]) -> (Result<usize, Error>, Scripted) {
        let list = tasks(names);
        let mut term = Scripted::new(keys);
        let result = run_picker(&mut term, "Select a task", &list);
        (result, term)
    }

    // -- navigation ---------------------------------------------------------

    #[test]
    fn enter_selects_the_first_task() {
        let (result, _) = press(&[Key::Enter], &["dev", "build", "test"]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn down_moves_the_cursor() {
        let (result, _) = press(
            &[Key::Down, Key::Down, Key::Enter],
            &["dev", "build", "test"],
        );
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn up_moves_back() {
        let (result, _) = press(
            &[Key::Down, Key::Down, Key::Up, Key::Enter],
            &["a", "b", "c"],
        );
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn up_at_the_top_stays_put() {
        let (result, _) = press(&[Key::Up, Key::Up, Key::Up, Key::Enter], &["a", "b", "c"]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn down_at_the_bottom_stays_put() {
        let keys = [Key::Down, Key::Down, Key::Down, Key::Down, Key::Enter];
        let (result, _) = press(&keys, &["a", "b", "c"]);
        assert_eq!(result.unwrap(), 2);
    }

    #[test]
    fn home_and_end_jump_to_the_ends() {
        let (result, _) = press(&[Key::End, Key::Enter], &["a", "b", "c"]);
        assert_eq!(result.unwrap(), 2);

        let (result, _) = press(&[Key::End, Key::Home, Key::Enter], &["a", "b", "c"]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn a_single_task_cannot_be_moved_off() {
        let keys = [Key::Down, Key::Up, Key::End, Key::Home, Key::Enter];
        let (result, _) = press(&keys, &["only"]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn ignored_keys_change_nothing() {
        let keys = [Key::Down, Key::Ignored, Key::Ignored, Key::Enter];
        let (result, _) = press(&keys, &["a", "b", "c"]);
        assert_eq!(result.unwrap(), 1);
    }

    // -- cancelling ---------------------------------------------------------

    #[test]
    fn cancel_returns_cancelled_and_clears_the_frame() {
        let (result, term) = press(&[Key::Cancel], &["a", "b"]);
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(term.cleared.len(), 1, "the drawn frame must be erased");
    }

    #[test]
    fn committing_clears_the_frame_too() {
        let (result, term) = press(&[Key::Down, Key::Enter], &["a", "b"]);
        assert!(result.is_ok());
        // Cleared by exactly the number of lines last drawn, or the erase
        // would eat unrelated scrollback.
        assert_eq!(
            term.cleared,
            vec![term.frames.last().unwrap().lines().count()]
        );
    }

    // -- filtering ----------------------------------------------------------

    /// The cursor indexes the filtered view, so committing has to map back to
    /// the original position. Getting this wrong runs the wrong script.
    #[test]
    fn filtering_returns_the_original_index() {
        let names = ["dev", "build", "test", "lint"];

        let mut keys = typed("bui");
        keys.push(Key::Enter);
        let (result, _) = press(&keys, &names);
        assert_eq!(result.unwrap(), 1, "should select build");

        let mut keys = typed("lin");
        keys.push(Key::Enter);
        let (result, _) = press(&keys, &names);
        assert_eq!(result.unwrap(), 3, "should select lint");
    }

    #[test]
    fn filtering_then_navigating_maps_back_correctly() {
        // "t" matches test and lint; Down picks the second of those.
        let mut keys = typed("t");
        keys.extend([Key::Down, Key::Enter]);
        let (result, _) = press(&keys, &["dev", "build", "test", "lint"]);
        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn filtering_is_case_insensitive() {
        let mut keys = typed("BUILD");
        keys.push(Key::Enter);
        let (result, _) = press(&keys, &["dev", "build"]);
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn backspace_widens_the_filter_again() {
        let mut keys = typed("build");
        keys.extend([Key::Backspace; 5]);
        keys.extend([Key::Down, Key::Enter]);
        let (result, term) = press(&keys, &["dev", "build", "test"]);

        assert_eq!(
            result.unwrap(),
            1,
            "full list restored, so Down lands on build"
        );
        assert_eq!(Scripted::rows(term.last_frame()).len(), 3);
    }

    #[test]
    fn backspace_on_an_empty_filter_is_harmless() {
        let keys = [Key::Backspace, Key::Backspace, Key::Enter];
        let (result, _) = press(&keys, &["a", "b"]);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn filtering_resets_the_cursor() {
        // Move down first; the filter must not leave the cursor out of range.
        let mut keys = vec![Key::Down, Key::Down];
        keys.extend(typed("dev"));
        keys.push(Key::Enter);
        let (result, _) = press(&keys, &["dev", "build", "test"]);
        assert_eq!(result.unwrap(), 0);
    }

    /// With nothing matching there is no cursor position, so Enter must not
    /// commit anything.
    #[test]
    fn enter_does_nothing_when_the_filter_matches_nothing() {
        let mut keys = typed("zzz");
        keys.push(Key::Enter);
        keys.push(Key::Enter);
        keys.push(Key::Cancel);
        let (result, term) = press(&keys, &["dev", "build"]);

        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(term.last_frame().contains("nothing matches"));
    }

    #[test]
    fn recovering_from_an_empty_filter_still_selects() {
        let mut keys = typed("zzz");
        keys.extend([Key::Backspace; 3]);
        keys.extend([Key::Down, Key::Enter]);
        let (result, _) = press(&keys, &["dev", "build"]);
        assert_eq!(result.unwrap(), 1);
    }

    // -- scrolling ----------------------------------------------------------

    #[test]
    fn the_window_scrolls_with_the_cursor() {
        let names: Vec<String> = (0..30).map(|i| format!("task{i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        // Walk past the bottom of the visible window.
        let mut keys = vec![Key::Down; 20];
        keys.push(Key::Enter);
        let (result, term) = press(&keys, &refs);

        assert_eq!(result.unwrap(), 20);

        let rows = Scripted::rows(term.last_frame());
        assert_eq!(rows.len(), MAX_ROWS, "the list is capped at MAX_ROWS");
        assert_eq!(
            rows.last().unwrap(),
            ">task20",
            "the cursor should be the bottom visible row"
        );
        assert_eq!(
            rows.first().unwrap(),
            "task06",
            "window should have scrolled"
        );
    }

    #[test]
    fn scrolling_back_up_moves_the_window_with_it() {
        let names: Vec<String> = (0..30).map(|i| format!("task{i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        let mut keys = vec![Key::Down; 20];
        keys.extend(vec![Key::Up; 18]);
        keys.push(Key::Enter);
        let (result, term) = press(&keys, &refs);

        assert_eq!(result.unwrap(), 2);
        let rows = Scripted::rows(term.last_frame());
        assert_eq!(rows.first().unwrap(), ">task02", "window follows back up");
    }

    #[test]
    fn end_on_a_long_list_shows_the_last_task() {
        let names: Vec<String> = (0..30).map(|i| format!("task{i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        let (result, term) = press(&[Key::End, Key::Enter], &refs);
        assert_eq!(result.unwrap(), 29);
        assert_eq!(Scripted::rows(term.last_frame()).last().unwrap(), ">task29");
    }

    #[test]
    fn a_long_list_reports_how_many_are_hidden() {
        let names: Vec<String> = (0..30).map(|i| format!("task{i:02}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();

        let (_, term) = press(&[Key::Enter], &refs);
        assert!(term
            .last_frame()
            .contains(&format!("... {} more", 30 - MAX_ROWS)));
    }

    // -- redraw bookkeeping -------------------------------------------------

    /// Each redraw moves the cursor up by the line count of the previous frame.
    /// If that count is wrong the picker eats scrollback or leaves debris.
    #[test]
    fn each_frame_rewinds_exactly_the_previous_frame() {
        let keys = [Key::Down, Key::Down, Key::Enter];
        let (_, term) = press(&keys, &["dev", "build", "test"]);

        for pair in term.frames.windows(2) {
            let previous_lines = pair[0].lines().count();
            assert!(
                pair[1].starts_with(&format!("\x1b[{previous_lines}A\x1b[0J")),
                "frame should rewind {previous_lines} lines, got {:?}",
                &pair[1][..pair[1].len().min(16)]
            );
        }
    }

    #[test]
    fn the_first_frame_does_not_rewind() {
        let (_, term) = press(&[Key::Enter], &["a"]);
        // It opens with the bold title, not a cursor-up and erase: there is
        // nothing drawn yet to rewind over, and doing so would eat scrollback.
        assert!(term.frames[0].starts_with("\x1b[1m"));
        assert!(!term.frames[0].contains("\x1b[0J"));
    }

    #[test]
    fn the_filter_is_shown_once_typing_starts() {
        let mut keys = typed("de");
        keys.push(Key::Enter);
        let (_, term) = press(&keys, &["dev", "build"]);

        assert!(
            !term.frames[0].contains("filter:"),
            "no filter line when empty"
        );
        assert!(term.last_frame().contains("filter: de"));
    }

    // -- key decoding -------------------------------------------------------

    /// decode_key is a pure function of what the kernel reports, so the whole
    /// mapping can be checked without a console.
    #[test]
    #[cfg(windows)]
    fn key_codes_decode_to_actions() {
        for (code, want) in [
            (vk::UP, Key::Up),
            (vk::DOWN, Key::Down),
            (vk::HOME, Key::Home),
            (vk::PRIOR, Key::Home),
            (vk::END, Key::End),
            (vk::NEXT, Key::End),
            (vk::RETURN, Key::Enter),
            (vk::ESCAPE, Key::Cancel),
            (vk::BACK, Key::Backspace),
        ] {
            assert_eq!(decode_key(code, 0), want, "virtual key {code:#x}");
        }
    }

    #[test]
    #[cfg(windows)]
    fn printable_characters_become_filter_input() {
        // An unrecognised virtual key falls through to the character.
        assert_eq!(decode_key(0x41, b'a' as u16), Key::Char('a'));
        assert_eq!(decode_key(0x20, b' ' as u16), Key::Char(' '));
        assert_eq!(decode_key(0xBE, b'.' as u16), Key::Char('.'));
        assert_eq!(decode_key(0, 'é' as u16), Key::Char('é'));
    }

    #[test]
    #[cfg(windows)]
    fn ctrl_c_cancels() {
        // ENABLE_PROCESSED_INPUT is off, so Ctrl+C arrives as a character
        // rather than terminating the process.
        assert_eq!(decode_key(0x43, 3), Key::Cancel);
    }

    #[test]
    #[cfg(windows)]
    fn other_control_characters_are_ignored() {
        for ch in [0u16, 1, 2, 4, 9, 27] {
            assert_eq!(decode_key(0, ch), Key::Ignored, "char {ch}");
        }
        // A modifier press on its own reports no character at all.
        assert_eq!(decode_key(0x10, 0), Key::Ignored); // shift
    }

    #[test]
    fn label_pads_name_and_appends_command() {
        // "dev" padded to 5, then the two separating spaces.
        assert_eq!(label(&task("dev", "vite"), 5, 200), "dev    vite");
    }

    #[test]
    fn label_includes_description_when_present() {
        let t = Task {
            name: "build".into(),
            command: "tsc".into(),
            description: "Compile".into(),
        };
        assert_eq!(label(&t, 5, 200), "build  tsc  (Compile)");
    }

    #[test]
    fn label_falls_back_to_description() {
        let t = Task {
            name: "all".into(),
            command: String::new(),
            description: "Everything".into(),
        };
        assert_eq!(label(&t, 3, 200), "all  Everything");
    }

    #[test]
    fn label_is_bare_name_when_nothing_else() {
        assert_eq!(label(&task("solo", ""), 4, 200), "solo");
    }

    #[test]
    fn label_truncates_to_width() {
        let t = task("dev", &"x".repeat(200));
        let got = label(&t, 3, 40);
        assert_eq!(got.chars().count(), 36); // width - 4
        assert!(got.ends_with('…'));
    }

    #[test]
    fn label_truncates_on_character_boundaries() {
        let t = task("i18n", &"é".repeat(100));
        let got = label(&t, 4, 30);
        assert!(got.is_char_boundary(got.len()));
        assert!(got.chars().all(|c| c != '\u{FFFD}'));
    }

    #[test]
    fn filter_matches_name_and_command() {
        let tasks = vec![
            task("dev", "vite"),
            task("build", "tsc"),
            task("test", "vitest"),
        ];

        assert_eq!(matching(&tasks, ""), vec![0, 1, 2]);
        assert_eq!(matching(&tasks, "dev"), vec![0]);
        // "vit" appears in the command of both dev and test.
        assert_eq!(matching(&tasks, "vit"), vec![0, 2]);
        assert_eq!(matching(&tasks, "BUILD"), vec![1]); // case-insensitive
        assert!(matching(&tasks, "nope").is_empty());
    }

    /// Pins the deliberate trade: case is folded for ASCII only.
    #[test]
    fn filter_folds_ascii_case_only() {
        let tasks = vec![task("déploy", "run"), task("ÉTAPE", "run")];

        // Non-ASCII text still matches when the case is the same...
        assert_eq!(matching(&tasks, "dépl"), vec![0]);
        assert_eq!(matching(&tasks, "ÉTA"), vec![1]);
        // ...and ASCII letters around it still fold.
        assert_eq!(matching(&tasks, "DÉPL"), Vec::<usize>::new());
        assert_eq!(matching(&tasks, "Éta"), vec![1]);
        // But non-ASCII letters themselves do not fold.
        assert!(matching(&tasks, "étape").is_empty());
    }

    #[test]
    fn contains_ignore_ascii_case_edges() {
        assert!(contains_ignore_ascii_case("anything", ""));
        assert!(!contains_ignore_ascii_case("", "a"));
        assert!(!contains_ignore_ascii_case("ab", "abc"));
        assert!(contains_ignore_ascii_case("Build:Prod", "d:p"));
    }
}
