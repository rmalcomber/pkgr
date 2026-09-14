package ui

import (
	"strings"
	"testing"
	"unsafe"

	"pkgr/internal/manifest"
)

func task(name, command string) manifest.Task {
	return manifest.Task{Name: name, Command: command}
}

func TestLabel(t *testing.T) {
	tests := []struct {
		name string
		task manifest.Task
		pad  int
		want string
	}{
		{
			name: "name and command are padded apart",
			task: task("dev", "vite"),
			pad:  5,
			want: "dev    vite", // padded to 5, then the two separating spaces
		},
		{
			name: "description is appended to the command",
			task: manifest.Task{Name: "build", Command: "tsc", Description: "Compile"},
			pad:  5,
			want: "build  tsc  (Compile)",
		},
		{
			name: "description alone is used when there is no command",
			task: manifest.Task{Name: "all", Description: "Everything"},
			pad:  3,
			want: "all  Everything",
		},
		{
			name: "bare name when there is nothing else to show",
			task: task("solo", ""),
			pad:  4,
			want: "solo",
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := label(tc.task, tc.pad, 200); got != tc.want {
				t.Errorf("label = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestLabelTruncates(t *testing.T) {
	got := label(task("dev", strings.Repeat("x", 200)), 3, 40)
	if n := len([]rune(got)); n != 36 { // width - 4
		t.Errorf("length = %d, want 36", n)
	}
	if !strings.HasSuffix(got, "\u2026") {
		t.Errorf("label = %q, want it to end in an ellipsis", got)
	}
}

// Truncation counts runes, so a multi-byte command must never be cut mid-character.
func TestLabelTruncatesOnRuneBoundaries(t *testing.T) {
	got := label(task("i18n", strings.Repeat("\u00e9", 100)), 4, 30)
	for i, r := range got {
		if r == '\uFFFD' {
			t.Fatalf("label %q has a broken rune at byte %d", got, i)
		}
	}
}

func TestNameWidth(t *testing.T) {
	tasks := []manifest.Task{{Name: "a"}, {Name: "abcdef"}, {Name: "abc"}}
	if got := nameWidth(tasks); got != 6 {
		t.Errorf("nameWidth = %d, want 6", got)
	}
}

func TestMatching(t *testing.T) {
	tasks := []manifest.Task{task("dev", "vite"), task("build", "tsc"), task("test", "vitest")}

	tests := []struct {
		filter string
		want   []int
	}{
		{"", []int{0, 1, 2}},
		{"dev", []int{0}},
		{"vit", []int{0, 2}}, // matches the command of dev and test
		{"BUILD", []int{1}},  // case-insensitive
		{"nope", nil},
	}

	for _, tc := range tests {
		t.Run(tc.filter, func(t *testing.T) {
			got := matching(tasks, tc.filter)
			if len(got) != len(tc.want) {
				t.Fatalf("matching(%q) = %v, want %v", tc.filter, got, tc.want)
			}
			for i := range got {
				if got[i] != tc.want[i] {
					t.Fatalf("matching(%q) = %v, want %v", tc.filter, got, tc.want)
				}
			}
		})
	}
}

// The frame must report its own height, since the next redraw moves the cursor
// up by exactly that many lines before clearing.
func TestRenderCountsItsOwnLines(t *testing.T) {
	tasks := []manifest.Task{task("a", "1"), task("b", "2"), task("c", "3")}
	shown := []int{0, 1, 2}

	frame, lines := render("Title", "", tasks, shown, 0, 0, len(shown), 1, 200, 0)
	if got := strings.Count(frame, "\n"); got != lines {
		t.Errorf("frame has %d newlines but reported %d lines", got, lines)
	}
	// title + 3 rows + hint
	if lines != 5 {
		t.Errorf("lines = %d, want 5", lines)
	}
}

func TestRenderCountsHiddenRowNotice(t *testing.T) {
	var tasks []manifest.Task
	for i := range 30 {
		tasks = append(tasks, task(string(rune('a'+i%26)), "cmd"))
	}
	shown := allIndexes(len(tasks))

	frame, lines := render("Title", "", tasks, shown, 0, 0, maxRows, 1, 200, 0)
	if got := strings.Count(frame, "\n"); got != lines {
		t.Errorf("frame has %d newlines but reported %d lines", got, lines)
	}
	if !strings.Contains(frame, "more") {
		t.Error("frame should mention the rows it could not show")
	}
	// title + maxRows + "... n more" + hint
	if lines != maxRows+3 {
		t.Errorf("lines = %d, want %d", lines, maxRows+3)
	}
}

func TestRenderEmptyFilterResult(t *testing.T) {
	tasks := []manifest.Task{task("a", "1")}

	frame, lines := render("Title", "zzz", tasks, nil, 0, 0, 0, 1, 200, 0)
	if got := strings.Count(frame, "\n"); got != lines {
		t.Errorf("frame has %d newlines but reported %d lines", got, lines)
	}
	if !strings.Contains(frame, "nothing matches") {
		t.Error("frame should say that nothing matched")
	}
}

// INPUT_RECORD is read straight out of the kernel, so its layout has to be
// exactly right or every keypress decodes to nonsense.
func TestInputRecordLayout(t *testing.T) {
	if got := unsafe.Sizeof(inputRecord{}); got != 20 {
		t.Errorf("sizeof(inputRecord) = %d, want 20", got)
	}

	var rec inputRecord
	base := uintptr(unsafe.Pointer(&rec))
	offsets := []struct {
		name string
		got  uintptr
		want uintptr
	}{
		{"eventType", uintptr(unsafe.Pointer(&rec.eventType)) - base, 0},
		{"keyDown", uintptr(unsafe.Pointer(&rec.keyDown)) - base, 4},
		{"repeat", uintptr(unsafe.Pointer(&rec.repeat)) - base, 8},
		{"keyCode", uintptr(unsafe.Pointer(&rec.keyCode)) - base, 10},
		{"scanCode", uintptr(unsafe.Pointer(&rec.scanCode)) - base, 12},
		{"char", uintptr(unsafe.Pointer(&rec.char)) - base, 14},
		{"ctrlState", uintptr(unsafe.Pointer(&rec.ctrlState)) - base, 16},
	}
	for _, o := range offsets {
		if o.got != o.want {
			t.Errorf("offset of %s = %d, want %d", o.name, o.got, o.want)
		}
	}
}
