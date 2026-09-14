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

/// Shows `tasks` and returns the index of the chosen one.
pub fn select(title: &str, tasks: &[Task]) -> Result<usize, Error> {
    if tasks.is_empty() {
        return Err(Error::Cancelled);
    }

    let mut term = Terminal::acquire()?;

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

        let mut frame = String::new();
        if painted > 0 {
            // Return to the top of the previous frame and wipe it.
            frame.push_str(&format!("\x1b[{painted}A"));
            frame.push_str("\x1b[0J");
        }

        frame.push_str(&format!("\x1b[1m{title}\x1b[0m"));
        if !filter.is_empty() {
            frame.push_str(&format!("  \x1b[2mfilter: {filter}\x1b[0m"));
        }
        frame.push('\n');
        let mut lines = 1;

        if shown.is_empty() {
            frame.push_str("  \x1b[2m(nothing matches)\x1b[0m\n");
            lines += 1;
        }

        for row in 0..rows {
            let idx = shown[scroll + row];
            let selected = scroll + row == cursor;
            let text = label(&tasks[idx], pad, width);

            if selected {
                // Reverse video reads correctly on any colour scheme.
                frame.push_str(&format!("\x1b[7m> {text}\x1b[0m\n"));
            } else {
                frame.push_str(&format!("  {text}\n"));
            }
            lines += 1;
        }

        let hidden = shown.len().saturating_sub(rows);
        if hidden > 0 {
            frame.push_str(&format!("  \x1b[2m... {hidden} more\x1b[0m\n"));
            lines += 1;
        }

        frame.push_str("\x1b[2m  up/down move · type to filter · enter run · esc cancel\x1b[0m\n");
        lines += 1;

        term.write(&frame)?;
        painted = lines;

        match term.read_key()? {
            Key::Enter => {
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

/// Case-insensitive substring match over the name and command, which is what
/// filtering a script list actually needs.
fn matching(tasks: &[Task], filter: &str) -> Vec<usize> {
    if filter.is_empty() {
        return (0..tasks.len()).collect();
    }
    let needle = filter.to_lowercase();
    (0..tasks.len())
        .filter(|&i| {
            tasks[i].name.to_lowercase().contains(&needle)
                || tasks[i].command.to_lowercase().contains(&needle)
        })
        .collect()
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
        self.out.write_all(s.as_bytes())?;
        self.out.flush()
    }

    /// Erases the drawn frame so the picker leaves no trace behind.
    fn clear(&mut self, lines: usize) -> io::Result<()> {
        if lines > 0 {
            self.write(&format!("\x1b[{lines}A\x1b[0J"))?;
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

                // Virtual key codes, which avoids parsing escape sequences.
                const VK_RETURN: u16 = 0x0D;
                const VK_ESCAPE: u16 = 0x1B;
                const VK_PRIOR: u16 = 0x21;
                const VK_NEXT: u16 = 0x22;
                const VK_END: u16 = 0x23;
                const VK_HOME: u16 = 0x24;
                const VK_UP: u16 = 0x26;
                const VK_DOWN: u16 = 0x28;
                const VK_BACK: u16 = 0x08;

                return Ok(match key.wVirtualKeyCode {
                    VK_UP => Key::Up,
                    VK_DOWN => Key::Down,
                    VK_HOME | VK_PRIOR => Key::Home,
                    VK_END | VK_NEXT => Key::End,
                    VK_RETURN => Key::Enter,
                    VK_ESCAPE => Key::Cancel,
                    VK_BACK => Key::Backspace,
                    _ => match char::from_u32(key.uChar.UnicodeChar as u32) {
                        // Ctrl+C arrives as an ordinary control character now
                        // that ENABLE_PROCESSED_INPUT is off.
                        Some('\u{3}') => Key::Cancel,
                        Some(c) if !c.is_control() => Key::Char(c),
                        _ => Key::Ignored,
                    },
                });
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
}
