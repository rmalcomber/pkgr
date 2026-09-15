//! Placeholder for the eventual Linux port.

use std::io;

use super::{Console, Error, Key};

pub(super) struct Terminal;

impl Terminal {
    pub(super) fn acquire() -> Result<Self, Error> {
        // A termios raw-mode implementation goes here when this is ported.
        Err(Error::NotInteractive)
    }
}

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
