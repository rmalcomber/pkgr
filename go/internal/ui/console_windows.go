//go:build windows

package ui

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

// The console API is reached through syscall's lazy DLL loading, which is
// standard library, so the picker needs no third-party dependency at all.
var (
	kernel32 = syscall.NewLazyDLL("kernel32.dll")

	procGetConsoleMode             = kernel32.NewProc("GetConsoleMode")
	procSetConsoleMode             = kernel32.NewProc("SetConsoleMode")
	procReadConsoleInput           = kernel32.NewProc("ReadConsoleInputW")
	procGetConsoleScreenBufferInfo = kernel32.NewProc("GetConsoleScreenBufferInfo")
)

// Input mode flags.
const (
	enableProcessedInput = 0x0001
	enableLineInput      = 0x0002
	enableEchoInput      = 0x0004
)

// Output mode flags. The value collides with enableEchoInput, but the two
// belong to different mode sets.
const enableVirtualTerminalProcessing = 0x0004

const eventKey = 0x0001

// Virtual key codes, which is why input is read as events rather than as ANSI
// escape sequences parsed back out of stdin.
const (
	vkBack   = 0x08
	vkReturn = 0x0D
	vkEscape = 0x1B
	vkPrior  = 0x21 // page up
	vkNext   = 0x22 // page down
	vkEnd    = 0x23
	vkHome   = 0x24
	vkUp     = 0x26
	vkDown   = 0x28
)

type coord struct {
	X, Y int16
}

type smallRect struct {
	Left, Top, Right, Bottom int16
}

type consoleScreenBufferInfo struct {
	Size              coord
	CursorPosition    coord
	Attributes        uint16
	Window            smallRect
	MaximumWindowSize coord
}

// inputRecord is INPUT_RECORD laid out for its KEY_EVENT case. The union's
// largest member is 16 bytes, so the whole record is 20 with the leading event
// type and its padding.
type inputRecord struct {
	eventType uint16
	_         uint16
	keyDown   int32
	repeat    uint16
	keyCode   uint16
	scanCode  uint16
	char      uint16
	ctrlState uint32
}

type terminal struct {
	in         syscall.Handle
	out        syscall.Handle
	inputMode  uint32
	outputMode uint32
}

// acquireTerminal switches the console into raw mode, or reports that there is
// no console to draw on.
func acquireTerminal() (*terminal, error) {
	t := &terminal{
		in:  syscall.Handle(os.Stdin.Fd()),
		out: syscall.Handle(os.Stdout.Fd()),
	}

	// GetConsoleMode fails on a redirected handle, which doubles as the
	// interactivity check: no console mode means nothing to draw on.
	if !getConsoleMode(t.in, &t.inputMode) || !getConsoleMode(t.out, &t.outputMode) {
		return nil, ErrNotInteractive
	}

	// Line input and echo would buffer keys until Enter and print them.
	// Dropping processed input delivers Ctrl+C as a key event so it cancels the
	// picker rather than killing the process.
	setConsoleMode(t.in, t.inputMode&^(enableLineInput|enableEchoInput|enableProcessedInput))
	setConsoleMode(t.out, t.outputMode|enableVirtualTerminalProcessing)

	t.write("\x1b[?25l") // hide the cursor while drawing
	return t, nil
}

// release restores the console exactly as it was found.
func (t *terminal) release() {
	t.write("\x1b[?25h")
	setConsoleMode(t.in, t.inputMode)
	setConsoleMode(t.out, t.outputMode)
}

func (t *terminal) width() int {
	var info consoleScreenBufferInfo
	r, _, _ := procGetConsoleScreenBufferInfo.Call(uintptr(t.out), uintptr(unsafe.Pointer(&info)))
	if r == 0 {
		return 80
	}
	if w := int(info.Window.Right-info.Window.Left) + 1; w > 0 {
		return w
	}
	return 80
}

func (t *terminal) write(s string) error {
	_, err := os.Stdout.WriteString(s)
	return err
}

// clear erases the drawn frame so the picker leaves no trace behind.
func (t *terminal) clear(lines int) {
	if lines > 0 {
		t.write(fmt.Sprintf("\x1b[%dA\x1b[0J", lines))
	}
}

// readKey blocks until a key is pressed and reports what it means.
func (t *terminal) readKey() (key, rune, error) {
	for {
		var rec inputRecord
		var read uint32

		r, _, err := procReadConsoleInput.Call(
			uintptr(t.in),
			uintptr(unsafe.Pointer(&rec)),
			1,
			uintptr(unsafe.Pointer(&read)),
		)
		if r == 0 {
			return keyIgnored, 0, fmt.Errorf("cannot read console input: %w", err)
		}
		if read == 0 || rec.eventType != eventKey || rec.keyDown == 0 {
			// Key-up events would otherwise double every press.
			continue
		}

		switch rec.keyCode {
		case vkUp:
			return keyUp, 0, nil
		case vkDown:
			return keyDown, 0, nil
		case vkHome, vkPrior:
			return keyHome, 0, nil
		case vkEnd, vkNext:
			return keyEnd, 0, nil
		case vkReturn:
			return keyEnter, 0, nil
		case vkEscape:
			return keyCancel, 0, nil
		case vkBack:
			return keyBackspace, 0, nil
		}

		// Ctrl+C arrives as an ordinary control character now that processed
		// input is off.
		switch c := rune(rec.char); {
		case c == 3:
			return keyCancel, 0, nil
		case c >= ' ':
			return keyRune, c, nil
		}
	}
}

func getConsoleMode(h syscall.Handle, mode *uint32) bool {
	r, _, _ := procGetConsoleMode.Call(uintptr(h), uintptr(unsafe.Pointer(mode)))
	return r != 0
}

func setConsoleMode(h syscall.Handle, mode uint32) {
	procSetConsoleMode.Call(uintptr(h), uintptr(mode))
}
