package manifest

import (
	"errors"
	"strings"
	"testing"
)

func scanNames(t *testing.T, src, key string) []string {
	t.Helper()
	c, err := scan([]byte(src), key)
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}
	out := make([]string, len(c.Tasks))
	for i, task := range c.Tasks {
		out[i] = task.Name
	}
	return out
}

func TestScanPreservesSourceOrder(t *testing.T) {
	src := `{
		"name": "demo",
		"scripts": { "zebra": "z", "build": "b", "alpha": "a" },
		"devDependencies": { "nested": { "deep": [1, 2, {"x": true}] } }
	}`
	if got, want := scanNames(t, src, "scripts"), []string{"zebra", "build", "alpha"}; !equal(got, want) {
		t.Errorf("order = %v, want %v", got, want)
	}
}

func TestScanReadsPackageManager(t *testing.T) {
	c, err := scan([]byte(`{"packageManager":"pnpm@9.1.0","scripts":{"dev":"vite"}}`), "scripts")
	if err != nil {
		t.Fatal(err)
	}
	if c.PackageManager != "pnpm@9.1.0" {
		t.Errorf("packageManager = %q, want pnpm@9.1.0", c.PackageManager)
	}
}

// A junk value here must not fail the whole parse: lockfile detection still
// gives a usable answer.
func TestScanToleratesNonStringPackageManager(t *testing.T) {
	c, err := scan([]byte(`{"packageManager":123,"scripts":{"dev":"vite"}}`), "scripts")
	if err != nil {
		t.Fatal(err)
	}
	if c.PackageManager != "" {
		t.Errorf("packageManager = %q, want empty", c.PackageManager)
	}
	if len(c.Tasks) != 1 {
		t.Errorf("tasks = %v, want the scripts to still be read", c.Tasks)
	}
}

func TestScanDenoObjectTasks(t *testing.T) {
	src := `{
		"tasks": {
			"build": { "description": "Bundle it", "command": "deno bundle main.ts" },
			"all": { "dependencies": ["build", "test"] },
			"test": "deno test -A"
		}
	}`
	c, err := scan([]byte(src), "tasks")
	if err != nil {
		t.Fatal(err)
	}

	if got, want := []string{c.Tasks[0].Name, c.Tasks[1].Name, c.Tasks[2].Name}, []string{"build", "all", "test"}; !equal(got, want) {
		t.Fatalf("tasks = %v, want %v", got, want)
	}
	if c.Tasks[0].Command != "deno bundle main.ts" {
		t.Errorf("build command = %q", c.Tasks[0].Command)
	}
	if c.Tasks[0].Description != "Bundle it" {
		t.Errorf("build description = %q", c.Tasks[0].Description)
	}
	if !strings.Contains(c.Tasks[1].Command, "build, test") {
		t.Errorf("all command = %q, want it to mention its dependencies", c.Tasks[1].Command)
	}
}

func TestScanCommentsAndTrailingCommas(t *testing.T) {
	src := "{\n // leading\n \"tasks\": {\n \"dev\": \"deno run -A main.ts\", // inline\n /* block\n comment */\n \"fmt\": \"deno fmt\",\n },\n}"
	if got, want := scanNames(t, src, "tasks"), []string{"dev", "fmt"}; !equal(got, want) {
		t.Errorf("tasks = %v, want %v", got, want)
	}
}

// This is the case that makes it safe to treat // as a comment everywhere: a
// URL inside a string is data, not a comment.
func TestScanStructuralBytesInsideStrings(t *testing.T) {
	src := `{"homepage":"https://x.com//docs","scripts":{"a":"echo }{,"},"z":1}`
	c, err := scan([]byte(src), "scripts")
	if err != nil {
		t.Fatal(err)
	}
	if len(c.Tasks) != 1 || c.Tasks[0].Command != "echo }{," {
		t.Errorf("tasks = %+v, want one task with command %q", c.Tasks, "echo }{,")
	}
}

func TestScanStringEscapes(t *testing.T) {
	src := `{"scripts":{"a":"say \"hi\"\n\tdone \\ A 😀"}}`
	c, err := scan([]byte(src), "scripts")
	if err != nil {
		t.Fatal(err)
	}
	want := "say \"hi\"\n\tdone \\ A \U0001F600"
	if c.Tasks[0].Command != want {
		t.Errorf("command = %q, want %q", c.Tasks[0].Command, want)
	}
}

func TestScanMultibyteUTF8(t *testing.T) {
	c, err := scan([]byte(`{"scripts":{"i18n":"echo héllo → 世界"}}`), "scripts")
	if err != nil {
		t.Fatal(err)
	}
	if want := "echo héllo → 世界"; c.Tasks[0].Command != want {
		t.Errorf("command = %q, want %q", c.Tasks[0].Command, want)
	}
}

func TestScanNoTasks(t *testing.T) {
	tests := []struct {
		name string
		src  string
		key  string
	}{
		{"key absent", `{"name":"x","dependencies":{}}`, "scripts"},
		{"key empty", `{"scripts":{}}`, "scripts"},
		// A deno.json read as npm must not pick up its tasks.
		{"wrong key for kind", `{"tasks":{"a":"b"}}`, "scripts"},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := scan([]byte(tc.src), tc.key); !errors.Is(err, ErrNoTasks) {
				t.Errorf("err = %v, want ErrNoTasks", err)
			}
		})
	}
}

func TestScanSyntaxErrors(t *testing.T) {
	tests := []struct {
		name string
		src  string
	}{
		{"top level is an array", `[1,2,3]`},
		{"missing value", `{"scripts":{"a":}}`},
		{"unterminated string", `{"scripts":{"a":"oops}`},
		{"unterminated object", `{"scripts":{"a":"b"`},
		{"bad escape", `{"scripts":{"a":"\q"}}`},
		{"short unicode escape", `{"scripts":{"a":"\u12"}}`},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			_, err := scan([]byte(tc.src), "scripts")
			if err == nil {
				t.Fatal("expected an error")
			}
			var se *syntaxError
			if !errors.As(err, &se) {
				t.Fatalf("err = %v (%T), want a syntaxError", err, err)
			}
			if se.at < 0 || se.at > len(tc.src) {
				t.Errorf("offset %d is outside the input", se.at)
			}
		})
	}
}

func TestScanSkipsEveryValueType(t *testing.T) {
	src := `{"a":[1,[2,{"b":"}"}]],"b":null,"c":true,"d":-1.5e3,"e":"x","scripts":{"x":"y"}}`
	if got, want := scanNames(t, src, "scripts"), []string{"x"}; !equal(got, want) {
		t.Errorf("tasks = %v, want %v", got, want)
	}
}

// Non-runnable task values are ignored rather than treated as failures.
func TestScanIgnoresNonRunnableTaskValues(t *testing.T) {
	src := `{"scripts":{"a":"ok","b":null,"c":42,"d":"fine"}}`
	if got, want := scanNames(t, src, "scripts"), []string{"a", "d"}; !equal(got, want) {
		t.Errorf("tasks = %v, want %v", got, want)
	}
}
