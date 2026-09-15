//! The unix terminal, via termios.
//!
//! Where the Windows console hands over decoded virtual key codes, a unix
//! terminal delivers raw bytes: arrows arrive as `ESC [ A` and a filter
//! character may span up to four bytes of UTF-8. So the work the kernel does
//! on Windows is done here by `decode_key`, and the bytes it reads come
//! through the `Bytes` trait rather than from stdin directly, which is what
//! lets the whole escape grammar be tested without a terminal.

use std::io;
use std::io::Write;

use super::{Console, Error, Key};

/// How long to wait for the rest of an escape sequence before concluding that
/// Esc was pressed on its own. Long enough for a terminal to deliver a whole
/// sequence, short enough that Esc still feels immediate.
const ESCAPE_TIMEOUT_MS: i32 = 50;

pub(super) struct Terminal {
    /// The mode to put the terminal back into, whatever happens.
    original: libc::termios,
    out: io::Stdout,
}

impl Terminal {
    /// Switches the terminal into raw mode, or reports that there is none.
    pub(super) fn acquire() -> Result<Self, Error> {
        unsafe {
            let mut original: libc::termios = std::mem::zeroed();

            // tcgetattr fails with ENOTTY on a redirected handle, which
            // doubles as the interactivity check, exactly as GetConsoleMode
            // does on Windows. Drawing goes to stdout, so that has to be a
            // terminal too.
            if libc::tcgetattr(libc::STDIN_FILENO, &mut original) != 0
                || libc::isatty(libc::STDOUT_FILENO) == 0
            {
                return Err(Error::NotInteractive);
            }

            let mut raw = original;
            // ICANON and ECHO would buffer keys until Enter and print them.
            // Dropping ISIG delivers Ctrl+C as a byte so it cancels the picker
            // instead of killing the process, which is what turning off
            // ENABLE_PROCESSED_INPUT buys on Windows.
            raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG);
            // IXON so Ctrl+S cannot freeze the display mid-draw, ICRNL so
            // Enter arrives as the \r it actually is.
            raw.c_iflag &= !(libc::IXON | libc::ICRNL);
            // OPOST deliberately stays on: frames end their lines with a bare
            // \n, which without it would step down a row without returning to
            // column 0 and shear the list into a diagonal.
            raw.c_cc[libc::VMIN] = 1; // block until at least one byte
            raw.c_cc[libc::VTIME] = 0; // ... with no timeout
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);

            let mut term = Terminal {
                original,
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

/// Bytes arriving from the terminal.
///
/// `next` blocks; `next_soon` gives up after [`ESCAPE_TIMEOUT_MS`], which is
/// the only thing separating a bare Esc from the start of a sequence.
trait Bytes {
    fn next(&mut self) -> io::Result<Option<u8>>;
    fn next_soon(&mut self) -> io::Result<Option<u8>>;
}

impl Bytes for Terminal {
    /// Reads straight from the file descriptor rather than through
    /// `io::stdin`, whose buffering would swallow the rest of an escape
    /// sequence where `poll` could no longer see it.
    fn next(&mut self) -> io::Result<Option<u8>> {
        let mut byte = 0u8;
        loop {
            let read = unsafe {
                libc::read(
                    libc::STDIN_FILENO,
                    &mut byte as *mut u8 as *mut libc::c_void,
                    1,
                )
            };
            match read {
                1 => return Ok(Some(byte)),
                0 => return Ok(None), // stdin closed
                _ => {
                    let e = io::Error::last_os_error();
                    // A window resize interrupts the read; it is not a failure.
                    if e.kind() != io::ErrorKind::Interrupted {
                        return Err(e);
                    }
                }
            }
        }
    }

    fn next_soon(&mut self) -> io::Result<Option<u8>> {
        let mut fd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut fd, 1, ESCAPE_TIMEOUT_MS) } <= 0 {
            return Ok(None);
        }
        self.next()
    }
}

/// Maps the next keypress on the wire to what it means to the picker.
///
/// The unix counterpart of the Windows `decode_key`: everything here is a
/// function of the bytes alone, so it can be driven by a scripted source.
fn decode_key<B: Bytes>(bytes: &mut B) -> Result<Key, Error> {
    let Some(byte) = bytes.next()? else {
        // Stdin closed, so nothing further can ever be picked.
        return Err(Error::Cancelled);
    };

    Ok(match byte {
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Backspace,
        // Ctrl+C arrives as an ordinary byte now that ISIG is off.
        0x03 => Key::Cancel,
        0x1b => escape(bytes)?,
        c if c < 0x20 => Key::Ignored,
        c => character(bytes, c)?,
    })
}

/// Everything following an ESC byte.
///
/// A bare Esc is Cancel. Anything else is a sequence, and its bytes have to be
/// consumed whether or not the picker understands it, or the tail would land
/// in the filter as stray characters.
fn escape<B: Bytes>(bytes: &mut B) -> Result<Key, Error> {
    match bytes.next_soon()? {
        None => Ok(Key::Cancel),
        Some(b'[') => csi(bytes),
        // SS3, which is how a terminal in application-cursor mode sends arrows.
        Some(b'O') => Ok(match bytes.next_soon()? {
            Some(final_byte) => final_key(final_byte, 0),
            None => Key::Ignored,
        }),
        // Alt+key. The picker has no use for it, and the key itself must not
        // reach the filter.
        Some(_) => Ok(Key::Ignored),
    }
}

/// Reads a CSI sequence through to its final byte.
///
/// Only the leading number is kept, which is all the `~` forms need. Running
/// to the final byte regardless means an unrecognised sequence — a mouse
/// report, a bracketed-paste marker — is swallowed whole rather than
/// partially.
fn csi<B: Bytes>(bytes: &mut B) -> Result<Key, Error> {
    let mut number = 0u16;
    loop {
        let Some(byte) = bytes.next_soon()? else {
            return Ok(Key::Ignored); // sequence cut short
        };
        match byte {
            b'0'..=b'9' => {
                number = number
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as u16)
            }
            // A separator: later parameters are modifier flags, not the key.
            0x20..=0x3f => number = 0,
            0x40..=0x7e => return Ok(final_key(byte, number)),
            _ => return Ok(Key::Ignored),
        }
    }
}

/// The final byte of a sequence, with the leading parameter for the `~` forms.
///
/// Page up and page down map to Home and End, matching what the Windows
/// decoder does with VK_PRIOR and VK_NEXT.
fn final_key(final_byte: u8, number: u16) -> Key {
    match final_byte {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'~' => match number {
            1 | 7 => Key::Home,
            4 | 8 => Key::End,
            5 => Key::Home, // page up
            6 => Key::End,  // page down
            _ => Key::Ignored,
        },
        // Left and right arrows, delete, and everything else the picker has no
        // use for.
        _ => Key::Ignored,
    }
}

/// Assembles one character from its lead byte and the continuation bytes it
/// announces.
///
/// Terminals deliver UTF-8, so a character typed into the filter may span up
/// to four bytes and has to arrive as one `char` rather than as mojibake.
fn character<B: Bytes>(bytes: &mut B, lead: u8) -> Result<Key, Error> {
    let extra = match lead {
        0x00..=0x7f => 0,
        0xc2..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf4 => 3,
        // A continuation byte on its own, or an overlong lead: not the start
        // of a character.
        _ => return Ok(Key::Ignored),
    };

    let mut buf = [lead, 0, 0, 0];
    for slot in buf.iter_mut().take(1 + extra).skip(1) {
        match bytes.next_soon()? {
            Some(byte) => *slot = byte,
            None => return Ok(Key::Ignored), // truncated character
        }
    }

    match std::str::from_utf8(&buf[..1 + extra]) {
        Ok(text) => Ok(text.chars().next().map_or(Key::Ignored, Key::Char)),
        Err(_) => Ok(Key::Ignored),
    }
}

impl Console for Terminal {
    fn width(&self) -> usize {
        unsafe {
            let mut size: libc::winsize = std::mem::zeroed();
            if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0
            {
                return size.ws_col as usize;
            }
        }
        80
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
        decode_key(self)
    }
}

impl Drop for Terminal {
    /// Restores the terminal however the picker exits. As on Windows this
    /// cannot cover `panic = "abort"` or a kill signal.
    fn drop(&mut self) {
        let _ = self.write("\x1b[?25h");
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A byte source fed from a fixed queue. `next_soon` runs dry exactly when
    /// the queue does, which is how a bare Esc — nothing following within the
    /// timeout — is modelled.
    struct Feed(std::collections::VecDeque<u8>);

    impl Feed {
        fn new(bytes: &[u8]) -> Self {
            Feed(bytes.iter().copied().collect())
        }
    }

    impl Bytes for Feed {
        fn next(&mut self) -> io::Result<Option<u8>> {
            Ok(self.0.pop_front())
        }
        fn next_soon(&mut self) -> io::Result<Option<u8>> {
            Ok(self.0.pop_front())
        }
    }

    fn key(bytes: &[u8]) -> Key {
        decode_key(&mut Feed::new(bytes)).expect("should decode")
    }

    // -- single bytes -------------------------------------------------------

    #[test]
    fn enter_and_backspace_decode() {
        assert_eq!(key(b"\r"), Key::Enter);
        assert_eq!(key(b"\n"), Key::Enter);
        assert_eq!(key(b"\x7f"), Key::Backspace); // most terminals
        assert_eq!(key(b"\x08"), Key::Backspace); // ... and the rest
    }

    /// ISIG is off, so Ctrl+C is a byte to be decoded rather than a signal.
    #[test]
    fn ctrl_c_cancels() {
        assert_eq!(key(b"\x03"), Key::Cancel);
    }

    #[test]
    fn other_control_bytes_are_ignored() {
        for byte in [0x01u8, 0x02, 0x04, 0x09, 0x0b, 0x1f] {
            assert_eq!(key(&[byte]), Key::Ignored, "byte {byte:#x}");
        }
    }

    #[test]
    fn a_closed_stdin_cancels() {
        assert!(matches!(
            decode_key(&mut Feed::new(b"")),
            Err(Error::Cancelled)
        ));
    }

    // -- escape sequences ---------------------------------------------------

    /// The ambiguity the timeout exists to resolve: ESC with nothing behind it
    /// is the Esc key, not a sequence that never arrived.
    #[test]
    fn a_bare_escape_cancels() {
        assert_eq!(key(b"\x1b"), Key::Cancel);
    }

    #[test]
    fn csi_arrows_decode() {
        assert_eq!(key(b"\x1b[A"), Key::Up);
        assert_eq!(key(b"\x1b[B"), Key::Down);
    }

    /// A terminal in application-cursor mode sends SS3 instead of CSI.
    #[test]
    fn ss3_arrows_decode() {
        assert_eq!(key(b"\x1bOA"), Key::Up);
        assert_eq!(key(b"\x1bOB"), Key::Down);
        assert_eq!(key(b"\x1bOH"), Key::Home);
        assert_eq!(key(b"\x1bOF"), Key::End);
    }

    /// Home and End have more spellings than any other key, and which one
    /// arrives depends on the terminal.
    #[test]
    fn every_home_and_end_spelling_decodes() {
        for seq in [&b"\x1b[H"[..], b"\x1b[1~", b"\x1b[7~"] {
            assert_eq!(key(seq), Key::Home, "{:?}", seq);
        }
        for seq in [&b"\x1b[F"[..], b"\x1b[4~", b"\x1b[8~"] {
            assert_eq!(key(seq), Key::End, "{:?}", seq);
        }
    }

    /// Page up and down jump to the ends, matching VK_PRIOR and VK_NEXT on
    /// Windows.
    #[test]
    fn page_keys_jump_to_the_ends() {
        assert_eq!(key(b"\x1b[5~"), Key::Home);
        assert_eq!(key(b"\x1b[6~"), Key::End);
    }

    #[test]
    fn unused_keys_are_ignored() {
        assert_eq!(key(b"\x1b[C"), Key::Ignored); // right
        assert_eq!(key(b"\x1b[D"), Key::Ignored); // left
        assert_eq!(key(b"\x1b[3~"), Key::Ignored); // delete
        assert_eq!(key(b"\x1b[2~"), Key::Ignored); // insert
        assert_eq!(key(b"\x1bOP"), Key::Ignored); // F1
    }

    /// A modified arrow still moves: the modifier parameter is dropped and the
    /// final byte decides.
    #[test]
    fn modified_arrows_still_move() {
        assert_eq!(key(b"\x1b[1;5A"), Key::Up); // Ctrl+Up
        assert_eq!(key(b"\x1b[1;2B"), Key::Down); // Shift+Down
    }

    /// The point of running to the final byte. If an unrecognised sequence
    /// were abandoned partway, its remaining bytes would be decoded as
    /// keystrokes and land in the filter.
    #[test]
    fn an_unrecognised_sequence_is_consumed_whole() {
        let mut feed = Feed::new(b"\x1b[<0;12;34Ma");
        assert_eq!(decode_key(&mut feed).unwrap(), Key::Ignored, "mouse report");
        assert_eq!(
            decode_key(&mut feed).unwrap(),
            Key::Char('a'),
            "the next key must be the 'a', not a fragment of the report"
        );
    }

    #[test]
    fn a_truncated_sequence_is_ignored() {
        assert_eq!(key(b"\x1b["), Key::Ignored);
        assert_eq!(key(b"\x1b[1"), Key::Ignored);
        assert_eq!(key(b"\x1bO"), Key::Ignored);
    }

    /// Alt+key must not leak the key itself into the filter.
    #[test]
    fn alt_combinations_are_ignored() {
        assert_eq!(key(b"\x1ba"), Key::Ignored);
    }

    // -- characters ---------------------------------------------------------

    #[test]
    fn printable_bytes_become_filter_input() {
        assert_eq!(key(b"a"), Key::Char('a'));
        assert_eq!(key(b" "), Key::Char(' '));
        assert_eq!(key(b":"), Key::Char(':'));
    }

    /// A multi-byte character has to be reassembled, or typing it would push
    /// several replacement characters into the filter.
    #[test]
    fn utf8_characters_are_reassembled() {
        assert_eq!(key("é".as_bytes()), Key::Char('é')); // two bytes
        assert_eq!(key("€".as_bytes()), Key::Char('€')); // three
        assert_eq!(key("😀".as_bytes()), Key::Char('😀')); // four
    }

    #[test]
    fn invalid_utf8_is_ignored() {
        assert_eq!(key(&[0x80]), Key::Ignored); // a stray continuation byte
        assert_eq!(key(&[0xff]), Key::Ignored); // never valid UTF-8
        assert_eq!(key(&[0xc3, 0x28]), Key::Ignored); // bad continuation
        assert_eq!(key(&[0xe2, 0x82]), Key::Ignored); // cut short
    }

    /// The whole character is taken from the wire, so the byte after it
    /// decodes as the next key.
    #[test]
    fn a_character_does_not_consume_the_next_key() {
        let mut feed = Feed::new("é\r".as_bytes());
        assert_eq!(decode_key(&mut feed).unwrap(), Key::Char('é'));
        assert_eq!(decode_key(&mut feed).unwrap(), Key::Enter);
    }
}
