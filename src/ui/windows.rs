//! The Windows console.
//!
//! Key events come from `ReadConsoleInputW`, which reports virtual key codes,
//! so arrow keys arrive as events rather than as ANSI escape sequences that
//! would have to be parsed back out of stdin — the job `unix.rs` has to do.
//! Drawing uses VT sequences, which the console supports once
//! `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is switched on.

use std::io;
use std::io::Write;

use super::{Console, Error, Key};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, ReadConsoleInputW, SetConsoleMode,
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, INPUT_RECORD, KEY_EVENT, STD_INPUT_HANDLE,
    STD_OUTPUT_HANDLE,
};

pub(super) struct Terminal {
    stdin: HANDLE,
    stdout: HANDLE,
    original_input_mode: u32,
    original_output_mode: u32,
    out: io::Stdout,
}

impl Terminal {
    /// Switches the console into raw mode, or reports that there is no console.
    pub(super) fn acquire() -> Result<Self, Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// decode_key is a pure function of what the kernel reports, so the whole
    /// mapping can be checked without a console.
    #[test]
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
    fn printable_characters_become_filter_input() {
        // An unrecognised virtual key falls through to the character.
        assert_eq!(decode_key(0x41, b'a' as u16), Key::Char('a'));
        assert_eq!(decode_key(0x20, b' ' as u16), Key::Char(' '));
        assert_eq!(decode_key(0xBE, b'.' as u16), Key::Char('.'));
        assert_eq!(decode_key(0, 'é' as u16), Key::Char('é'));
    }

    #[test]
    fn ctrl_c_cancels() {
        // ENABLE_PROCESSED_INPUT is off, so Ctrl+C arrives as a character
        // rather than terminating the process.
        assert_eq!(decode_key(0x43, 3), Key::Cancel);
    }

    #[test]
    fn other_control_characters_are_ignored() {
        for ch in [0u16, 1, 2, 4, 9, 27] {
            assert_eq!(decode_key(0, ch), Key::Ignored, "char {ch}");
        }
        // A modifier press on its own reports no character at all.
        assert_eq!(decode_key(0x10, 0), Key::Ignored); // shift
    }
}
