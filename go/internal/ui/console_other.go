//go:build !windows

package ui

// A termios raw-mode implementation goes here when this is ported to Linux.
// Everything except the picker already works there.

type terminal struct{}

func acquireTerminal() (*terminal, error) { return nil, ErrNotInteractive }

func (t *terminal) release()           {}
func (t *terminal) width() int         { return 80 }
func (t *terminal) write(string) error { return nil }
func (t *terminal) clear(int)          {}
func (t *terminal) readKey() (key, rune, error) {
	return keyIgnored, 0, ErrCancelled
}
