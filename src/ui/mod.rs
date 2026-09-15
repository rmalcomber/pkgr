//! A hand-rolled select prompt.
//!
//! The picker loop, its rendering and its filtering live here and know nothing
//! about the operating system: they talk to a terminal only through the
//! `Console` trait. Each platform module supplies the one `Terminal` that
//! implements it — `windows.rs` against the Win32 console, `unix.rs` against
//! termios — so a platform port is a new file rather than a new branch in the
//! loop.

use std::io;

use crate::json::Task;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::Terminal;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix::Terminal;

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
///
/// Both platform modules decode into this, so the loop never sees a virtual
/// key code or an escape sequence.
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
