package manifest

import (
	"fmt"
	"strings"
	"unicode/utf16"
	"unicode/utf8"
)

// A targeted JSONC scanner.
//
// encoding/json would decode the whole document, and decoding into a map would
// lose the source order that drives the picker. Recovering that order means a
// streaming decoder walking every value anyway.
//
// pkgr only ever wants two things out of a manifest: one named object of tasks,
// and one optional string. So this walks the top level, keeps those, and skips
// everything else without allocating for it. Source order falls out for free,
// because tasks are appended as they are encountered.
//
// Comments and trailing commas are handled inline as whitespace, which
// deno.jsonc needs and hand-edited files sometimes have.

// scanner walks a JSONC document one byte at a time.
type scanner struct {
	b []byte
	i int
}

// syntaxError carries an offset so a malformed manifest can be pointed at.
type syntaxError struct {
	at  int
	msg string
}

func (e *syntaxError) Error() string {
	return fmt.Sprintf("%s at byte %d", e.msg, e.at)
}

func (s *scanner) errf(format string, args ...any) error {
	return &syntaxError{at: s.i, msg: fmt.Sprintf(format, args...)}
}

// scan reads src, collecting the tasks under taskKey ("scripts" or "tasks")
// along with the packageManager field.
func scan(src []byte, taskKey string) (Contents, error) {
	s := &scanner{b: src}
	var c Contents

	s.skipTrivia()
	if err := s.expect('{'); err != nil {
		return c, err
	}

	for {
		s.skipTrivia()
		if s.eat('}') {
			break
		}

		key, err := s.parseString()
		if err != nil {
			return c, err
		}
		s.skipTrivia()
		if err := s.expect(':'); err != nil {
			return c, err
		}
		s.skipTrivia()

		switch {
		case key == taskKey:
			if c.Tasks, err = s.parseTasks(); err != nil {
				return c, err
			}
		case key == "packageManager" && s.peek() == '"':
			if c.PackageManager, err = s.parseString(); err != nil {
				return c, err
			}
		default:
			// A non-string packageManager is odd but survivable: lockfile
			// detection still applies, so skip it rather than failing.
			if err := s.skipValue(); err != nil {
				return c, err
			}
		}

		s.skipTrivia()
		if !s.eat(',') {
			s.skipTrivia()
			if err := s.expect('}'); err != nil {
				return c, err
			}
			break
		}
	}

	if len(c.Tasks) == 0 {
		return c, ErrNoTasks
	}
	return c, nil
}

func (s *scanner) peek() byte {
	if s.i < len(s.b) {
		return s.b[s.i]
	}
	return 0
}

func (s *scanner) at(offset int) byte {
	if s.i+offset < len(s.b) {
		return s.b[s.i+offset]
	}
	return 0
}

func (s *scanner) eat(want byte) bool {
	if s.i < len(s.b) && s.b[s.i] == want {
		s.i++
		return true
	}
	return false
}

func (s *scanner) expect(want byte) error {
	if s.eat(want) {
		return nil
	}
	return s.errf("expected %q", want)
}

// skipTrivia consumes whitespace and comments. JSON proper has no comments, but
// Deno accepts them, so they are treated as whitespace here.
func (s *scanner) skipTrivia() {
	for s.i < len(s.b) {
		switch c := s.b[s.i]; {
		case c == ' ' || c == '\t' || c == '\r' || c == '\n':
			s.i++
		case c == '/' && s.at(1) == '/':
			for s.i < len(s.b) && s.b[s.i] != '\n' {
				s.i++
			}
		case c == '/' && s.at(1) == '*':
			s.i += 2
			for s.i < len(s.b) {
				if s.b[s.i] == '*' && s.at(1) == '/' {
					s.i += 2
					break
				}
				s.i++
			}
		default:
			return
		}
	}
}

// parseString reads a JSON string, resolving escapes.
func (s *scanner) parseString() (string, error) {
	if err := s.expect('"'); err != nil {
		return "", err
	}

	var out strings.Builder
	for {
		if s.i >= len(s.b) {
			return "", s.errf("unterminated string")
		}
		c := s.b[s.i]
		s.i++

		switch c {
		case '"':
			return out.String(), nil
		case '\\':
			if s.i >= len(s.b) {
				return "", s.errf("unterminated escape")
			}
			esc := s.b[s.i]
			s.i++
			switch esc {
			case '"':
				out.WriteByte('"')
			case '\\':
				out.WriteByte('\\')
			case '/':
				out.WriteByte('/')
			case 'b':
				out.WriteByte('\b')
			case 'f':
				out.WriteByte('\f')
			case 'n':
				out.WriteByte('\n')
			case 'r':
				out.WriteByte('\r')
			case 't':
				out.WriteByte('\t')
			case 'u':
				r, err := s.parseUnicodeEscape()
				if err != nil {
					return "", err
				}
				out.WriteRune(r)
			default:
				return "", s.errf("bad escape %q", esc)
			}
		default:
			// Multi-byte UTF-8 is copied through whole, so a character is
			// never split across writes.
			s.i--
			r, size := utf8.DecodeRune(s.b[s.i:])
			if r == utf8.RuneError && size <= 1 {
				return "", s.errf("invalid UTF-8 in string")
			}
			out.WriteRune(r)
			s.i += size
		}
	}
}

// parseUnicodeEscape handles \uXXXX, including the surrogate pairs that encode
// characters outside the basic multilingual plane.
func (s *scanner) parseUnicodeEscape() (rune, error) {
	first, err := s.parseHex4()
	if err != nil {
		return 0, err
	}

	// A high surrogate is only half a character; the low half follows.
	if utf16.IsSurrogate(rune(first)) {
		if s.peek() == '\\' && s.at(1) == 'u' {
			s.i += 2
			second, err := s.parseHex4()
			if err != nil {
				return 0, err
			}
			if r := utf16.DecodeRune(rune(first), rune(second)); r != utf8.RuneError {
				return r, nil
			}
		}
		return 0, s.errf("unpaired surrogate")
	}

	return rune(first), nil
}

func (s *scanner) parseHex4() (uint32, error) {
	var n uint32
	for range 4 {
		if s.i >= len(s.b) {
			return 0, s.errf("short \\u escape")
		}
		c := s.b[s.i]
		var d uint32
		switch {
		case c >= '0' && c <= '9':
			d = uint32(c - '0')
		case c >= 'a' && c <= 'f':
			d = uint32(c-'a') + 10
		case c >= 'A' && c <= 'F':
			d = uint32(c-'A') + 10
		default:
			return 0, s.errf("bad hex digit")
		}
		n = n*16 + d
		s.i++
	}
	return n, nil
}

// skipValue consumes any value without interpreting it.
func (s *scanner) skipValue() error {
	s.skipTrivia()
	switch c := s.peek(); {
	case s.i >= len(s.b):
		return s.errf("unexpected end of input")
	case c == '"':
		_, err := s.parseString()
		return err
	case c == '{' || c == '[':
		return s.skipContainer()
	default:
		// Numbers, true, false, null: run to the next structural byte.
		start := s.i
		for s.i < len(s.b) {
			switch s.b[s.i] {
			case ',', '}', ']', ' ', '\t', '\r', '\n', '/':
				goto done
			}
			s.i++
		}
	done:
		if s.i == start {
			// Consuming nothing means there was no value here at all, e.g.
			// `{"a":}`. Left unchecked this would look like an absent key
			// rather than malformed input.
			return s.errf("expected a value")
		}
		return nil
	}
}

// skipContainer skips a balanced object or array, ignoring structural bytes
// that appear inside strings or comments.
func (s *scanner) skipContainer() error {
	depth := 0
	for {
		s.skipTrivia()
		if s.i >= len(s.b) {
			return s.errf("unterminated object or array")
		}

		switch s.b[s.i] {
		case '{', '[':
			depth++
			s.i++
		case '}', ']':
			depth--
			s.i++
			if depth == 0 {
				return nil
			}
		case '"':
			if _, err := s.parseString(); err != nil {
				return err
			}
		default:
			s.i++
		}
	}
}

// parseTasks reads the task object, preserving key order.
func (s *scanner) parseTasks() ([]Task, error) {
	s.skipTrivia()
	if err := s.expect('{'); err != nil {
		return nil, err
	}

	var tasks []Task
	for {
		s.skipTrivia()
		if s.eat('}') {
			return tasks, nil
		}

		name, err := s.parseString()
		if err != nil {
			return nil, err
		}
		s.skipTrivia()
		if err := s.expect(':'); err != nil {
			return nil, err
		}
		s.skipTrivia()

		switch s.peek() {
		case '"':
			command, err := s.parseString()
			if err != nil {
				return nil, err
			}
			tasks = append(tasks, Task{Name: name, Command: command})
		case '{':
			task, err := s.parseTaskObject(name)
			if err != nil {
				return nil, fmt.Errorf("task %q: %w", name, err)
			}
			tasks = append(tasks, task)
		default:
			// Anything else is not runnable; ignore it rather than failing.
			if err := s.skipValue(); err != nil {
				return nil, err
			}
		}

		s.skipTrivia()
		if !s.eat(',') {
			s.skipTrivia()
			if err := s.expect('}'); err != nil {
				return nil, err
			}
			return tasks, nil
		}
	}
}

// parseTaskObject reads Deno's object form: a command, an optional description,
// and optional task dependencies.
func (s *scanner) parseTaskObject(name string) (Task, error) {
	var task Task
	task.Name = name

	if err := s.expect('{'); err != nil {
		return task, err
	}

	var dependencies []string
	for {
		s.skipTrivia()
		if s.eat('}') {
			break
		}

		key, err := s.parseString()
		if err != nil {
			return task, err
		}
		s.skipTrivia()
		if err := s.expect(':'); err != nil {
			return task, err
		}
		s.skipTrivia()

		switch {
		case key == "command" && s.peek() == '"':
			task.Command, err = s.parseString()
		case key == "description" && s.peek() == '"':
			task.Description, err = s.parseString()
		case key == "dependencies" && s.peek() == '[':
			dependencies, err = s.parseStringArray()
		default:
			err = s.skipValue()
		}
		if err != nil {
			return task, err
		}

		s.skipTrivia()
		if !s.eat(',') {
			s.skipTrivia()
			if err := s.expect('}'); err != nil {
				return task, err
			}
			break
		}
	}

	// A dependencies-only task is still runnable, so give it a readable row.
	if task.Command == "" && len(dependencies) > 0 {
		task.Command = "depends on: " + strings.Join(dependencies, ", ")
	}
	return task, nil
}

func (s *scanner) parseStringArray() ([]string, error) {
	if err := s.expect('['); err != nil {
		return nil, err
	}

	var out []string
	for {
		s.skipTrivia()
		if s.eat(']') {
			return out, nil
		}

		if s.peek() == '"' {
			item, err := s.parseString()
			if err != nil {
				return nil, err
			}
			out = append(out, item)
		} else if err := s.skipValue(); err != nil {
			return nil, err
		}

		s.skipTrivia()
		if !s.eat(',') {
			s.skipTrivia()
			if err := s.expect(']'); err != nil {
				return nil, err
			}
			return out, nil
		}
	}
}
