// Package ui renders the interactive task picker.
//
// The terminal handling is done directly rather than through a TUI library.
// Everything platform-specific lives behind the terminal type in
// console_windows.go; this file holds the parts that do not care which OS they
// are on.
package ui

import (
	"errors"
	"fmt"
	"strings"

	"pkgr/internal/manifest"
)

// ErrCancelled reports that the user dismissed the picker with Esc or Ctrl+C.
// Callers should exit quietly rather than treating it as a failure.
var ErrCancelled = errors.New("selection cancelled")

// ErrNotInteractive reports that there is no terminal to draw the picker on.
var ErrNotInteractive = errors.New("pkgr needs an interactive terminal to choose a task")

// maxRows caps how many task rows are visible at once, so a project with dozens
// of scripts scrolls in place instead of flooding the scrollback.
const maxRows = 15

// key is what a keypress means to the picker, decoded from whatever the
// platform reports.
type key int

const (
	keyIgnored key = iota
	keyUp
	keyDown
	keyHome
	keyEnd
	keyEnter
	keyCancel
	keyBackspace
	keyRune
)

// SelectTask shows tasks in source order and returns the chosen one.
func SelectTask(title string, tasks []manifest.Task) (manifest.Task, error) {
	if len(tasks) == 0 {
		return manifest.Task{}, errors.New("no tasks to choose from")
	}

	term, err := acquireTerminal()
	if err != nil {
		return manifest.Task{}, err
	}
	// Restores the console however this returns, panics included.
	defer term.release()

	var (
		shown   = allIndexes(len(tasks)) // tasks passing the current filter
		filter  string
		cursor  int // position within shown
		scroll  int // index of the first visible row
		painted int // lines drawn last frame, so they can be cleared
	)

	width := term.width()
	pad := nameWidth(tasks)

	for {
		rows := min(len(shown), maxRows)

		// Keep the cursor inside the visible window.
		if cursor < scroll {
			scroll = cursor
		} else if rows > 0 && cursor >= scroll+rows {
			scroll = cursor + 1 - rows
		}

		frame, lines := render(title, filter, tasks, shown, cursor, scroll, rows, pad, width, painted)
		if err := term.write(frame); err != nil {
			return manifest.Task{}, err
		}
		painted = lines

		k, r, err := term.readKey()
		if err != nil {
			return manifest.Task{}, err
		}

		switch k {
		case keyEnter:
			if cursor < len(shown) {
				term.clear(painted)
				return tasks[shown[cursor]], nil
			}
		case keyCancel:
			term.clear(painted)
			return manifest.Task{}, ErrCancelled
		case keyUp:
			if cursor > 0 {
				cursor--
			}
		case keyDown:
			if cursor+1 < len(shown) {
				cursor++
			}
		case keyHome:
			cursor = 0
		case keyEnd:
			cursor = max(len(shown)-1, 0)
		case keyBackspace:
			if runes := []rune(filter); len(runes) > 0 {
				filter = string(runes[:len(runes)-1])
				shown, cursor, scroll = matching(tasks, filter), 0, 0
			}
		case keyRune:
			filter += string(r)
			shown, cursor, scroll = matching(tasks, filter), 0, 0
		}
	}
}

// render builds one frame and reports how many lines it occupies, so the next
// frame knows how far up to move before redrawing.
func render(
	title, filter string,
	tasks []manifest.Task,
	shown []int,
	cursor, scroll, rows, pad, width, painted int,
) (string, int) {
	var b strings.Builder

	if painted > 0 {
		// Return to the top of the previous frame and wipe it.
		fmt.Fprintf(&b, "\x1b[%dA\x1b[0J", painted)
	}

	fmt.Fprintf(&b, "\x1b[1m%s\x1b[0m", title)
	if filter != "" {
		fmt.Fprintf(&b, "  \x1b[2mfilter: %s\x1b[0m", filter)
	}
	b.WriteString("\n")
	lines := 1

	if len(shown) == 0 {
		b.WriteString("  \x1b[2m(nothing matches)\x1b[0m\n")
		lines++
	}

	for row := range rows {
		task := tasks[shown[scroll+row]]
		text := label(task, pad, width)

		if scroll+row == cursor {
			// Reverse video reads correctly on any colour scheme.
			fmt.Fprintf(&b, "\x1b[7m> %s\x1b[0m\n", text)
		} else {
			fmt.Fprintf(&b, "  %s\n", text)
		}
		lines++
	}

	if hidden := len(shown) - rows; hidden > 0 {
		fmt.Fprintf(&b, "  \x1b[2m... %d more\x1b[0m\n", hidden)
		lines++
	}

	b.WriteString("\x1b[2m  up/down move · type to filter · enter run · esc cancel\x1b[0m\n")
	lines++

	return b.String(), lines
}

func allIndexes(n int) []int {
	out := make([]int, n)
	for i := range out {
		out[i] = i
	}
	return out
}

// matching is a case-insensitive substring match over name and command, which
// is what filtering a script list actually needs.
func matching(tasks []manifest.Task, filter string) []int {
	if filter == "" {
		return allIndexes(len(tasks))
	}

	needle := strings.ToLower(filter)
	var out []int
	for i, t := range tasks {
		if strings.Contains(strings.ToLower(t.Name), needle) ||
			strings.Contains(strings.ToLower(t.Command), needle) {
			out = append(out, i)
		}
	}
	return out
}

// label renders one row as "name    command", with names padded so the commands
// line up, and the whole row clipped to the terminal.
func label(t manifest.Task, pad, width int) string {
	detail := t.Command
	if detail == "" {
		detail = t.Description
	} else if t.Description != "" {
		detail = fmt.Sprintf("%s  (%s)", detail, t.Description)
	}

	if detail == "" {
		return t.Name
	}

	row := fmt.Sprintf("%-*s  %s", pad, t.Name, detail)

	// Leave room for the "> " marker. Truncation counts runes so a multi-byte
	// character is never cut in half.
	limit := width - 4
	if runes := []rune(row); limit > 1 && len(runes) > limit {
		row = string(runes[:limit-1]) + "…"
	}
	return row
}

func nameWidth(tasks []manifest.Task) int {
	widest := 0
	for _, t := range tasks {
		if n := len([]rune(t.Name)); n > widest {
			widest = n
		}
	}
	return widest
}
