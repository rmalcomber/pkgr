package manifest

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// names flattens tasks to their names so order assertions read clearly.
func names(tasks []Task) []string {
	out := make([]string, len(tasks))
	for i, t := range tasks {
		out[i] = t.Name
	}
	return out
}

func mustResolve(t *testing.T, dir string) Manifest {
	t.Helper()
	m, err := Resolve(dir)
	if err != nil {
		t.Fatalf("Resolve(%q) failed: %v", dir, err)
	}
	return m
}

func TestResolveDirectory(t *testing.T) {
	tests := []struct {
		dir      string
		wantBase string
		wantKind Kind
	}{
		{"testdata/npm", "package.json", KindNPM},
		{"testdata/deno-string", "deno.json", KindDeno},
		{"testdata/jsonc", "deno.jsonc", KindDeno},
	}

	for _, tc := range tests {
		t.Run(tc.dir, func(t *testing.T) {
			m := mustResolve(t, tc.dir)
			if got := filepath.Base(m.Path); got != tc.wantBase {
				t.Errorf("resolved to %s, want %s", got, tc.wantBase)
			}
			if m.Kind != tc.wantKind {
				t.Errorf("kind = %v, want %v", m.Kind, tc.wantKind)
			}
			if !filepath.IsAbs(m.Path) {
				t.Errorf("path %q is not absolute", m.Path)
			}
			if m.Dir != filepath.Dir(m.Path) {
				t.Errorf("dir = %q, want %q", m.Dir, filepath.Dir(m.Path))
			}
		})
	}
}

// package.json is preferred when a directory holds more than one manifest.
func TestResolvePrefersPackageJSON(t *testing.T) {
	dir := t.TempDir()
	for _, name := range []string{"package.json", "deno.json"} {
		if err := os.WriteFile(filepath.Join(dir, name), []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	m := mustResolve(t, dir)
	if filepath.Base(m.Path) != "package.json" {
		t.Errorf("resolved to %s, want package.json", filepath.Base(m.Path))
	}
}

func TestResolveExplicitFile(t *testing.T) {
	m := mustResolve(t, "testdata/deno-object/deno.json")
	if m.Kind != KindDeno {
		t.Errorf("kind = %v, want KindDeno", m.Kind)
	}
	if filepath.Base(m.Dir) != "deno-object" {
		t.Errorf("dir = %q, want it to end in deno-object", m.Dir)
	}
}

func TestResolveEmptyArgUsesWorkingDir(t *testing.T) {
	t.Chdir("testdata/npm")

	m := mustResolve(t, "")
	if filepath.Base(m.Path) != "package.json" {
		t.Errorf("resolved to %s, want package.json", m.Path)
	}
}

func TestResolveErrors(t *testing.T) {
	t.Run("no manifest in directory", func(t *testing.T) {
		if _, err := Resolve(t.TempDir()); !errors.Is(err, ErrNotFound) {
			t.Errorf("err = %v, want ErrNotFound", err)
		}
	})

	t.Run("missing path", func(t *testing.T) {
		_, err := Resolve(filepath.Join(t.TempDir(), "nope"))
		if err == nil {
			t.Fatal("expected an error")
		}
		if errors.Is(err, ErrNotFound) {
			t.Errorf("err = %v, want a stat error rather than ErrNotFound", err)
		}
	})

	t.Run("unsupported file name", func(t *testing.T) {
		dir := t.TempDir()
		path := filepath.Join(dir, "tsconfig.json")
		if err := os.WriteFile(path, []byte("{}"), 0o644); err != nil {
			t.Fatal(err)
		}
		_, err := Resolve(path)
		if err == nil || !strings.Contains(err.Error(), "not a supported manifest") {
			t.Errorf("err = %v, want an unsupported-manifest error", err)
		}
	})
}

// Source order drives the picker, so it must survive parsing intact.
func TestParsePreservesSourceOrder(t *testing.T) {
	c, err := Parse(mustResolve(t, "testdata/npm"))
	if err != nil {
		t.Fatal(err)
	}

	want := []string{"zebra", "build", "alpha", "test"}
	if got := names(c.Tasks); !equal(got, want) {
		t.Errorf("task order = %v, want %v", got, want)
	}
	if c.Tasks[1].Command != "tsc -p ." {
		t.Errorf("build command = %q, want %q", c.Tasks[1].Command, "tsc -p .")
	}
	if c.PackageManager != "pnpm@9.1.0" {
		t.Errorf("packageManager = %q, want pnpm@9.1.0", c.PackageManager)
	}
}

func TestParseDenoStringTasks(t *testing.T) {
	c, err := Parse(mustResolve(t, "testdata/deno-string"))
	if err != nil {
		t.Fatal(err)
	}

	if got, want := names(c.Tasks), []string{"dev", "check"}; !equal(got, want) {
		t.Errorf("tasks = %v, want %v", got, want)
	}
	if c.Tasks[0].Command != "deno run --watch main.ts" {
		t.Errorf("dev command = %q", c.Tasks[0].Command)
	}
}

func TestParseDenoObjectTasks(t *testing.T) {
	c, err := Parse(mustResolve(t, "testdata/deno-object"))
	if err != nil {
		t.Fatal(err)
	}

	if got, want := names(c.Tasks), []string{"build", "all", "test"}; !equal(got, want) {
		t.Fatalf("tasks = %v, want %v", got, want)
	}
	if c.Tasks[0].Command != "deno bundle main.ts" {
		t.Errorf("build command = %q", c.Tasks[0].Command)
	}
	if c.Tasks[0].Description != "Bundle the app" {
		t.Errorf("build description = %q", c.Tasks[0].Description)
	}
	// A dependencies-only task has no command; it still needs a readable row.
	if !strings.Contains(c.Tasks[1].Command, "build, test") {
		t.Errorf("all command = %q, want it to mention its dependencies", c.Tasks[1].Command)
	}
}

func TestParseJSONCWithCommentsAndTrailingCommas(t *testing.T) {
	c, err := Parse(mustResolve(t, "testdata/jsonc"))
	if err != nil {
		t.Fatal(err)
	}

	if got, want := names(c.Tasks), []string{"dev", "fmt"}; !equal(got, want) {
		t.Errorf("tasks = %v, want %v", got, want)
	}
	if c.Tasks[0].Command != "deno run -A main.ts" {
		t.Errorf("dev command = %q", c.Tasks[0].Command)
	}
}

func TestParseNoTasks(t *testing.T) {
	if _, err := Parse(mustResolve(t, "testdata/no-scripts")); !errors.Is(err, ErrNoTasks) {
		t.Errorf("err = %v, want ErrNoTasks", err)
	}
}

func TestPackageManagerDetection(t *testing.T) {
	tests := []struct {
		dir  string
		want string
	}{
		{"testdata/npm", "pnpm"},         // packageManager field
		{"testdata/pm-pnpm", "pnpm"},     // lockfile only
		{"testdata/pm-yarn", "yarn"},     // lockfile only
		{"testdata/pm-bun", "bun"},       // lockfile only
		{"testdata/pm-npm", "npm"},       // lockfile only
		{"testdata/bare", "npm"},         // no signal at all
		{"testdata/pm-bad-field", "npm"}, // junk field falls through to the lockfile
		{"testdata/deno-string", "deno"}, // deno manifests never consult lockfiles
	}

	for _, tc := range tests {
		t.Run(tc.dir, func(t *testing.T) {
			m := mustResolve(t, tc.dir)
			c, err := Parse(m)
			if err != nil {
				t.Fatal(err)
			}
			if got := PackageManager(m, c); got != tc.want {
				t.Errorf("PackageManager = %q, want %q", got, tc.want)
			}
		})
	}
}

func TestCommand(t *testing.T) {
	tests := []struct {
		dir      string
		task     string
		wantBin  string
		wantArgs []string
	}{
		{"testdata/npm", "build", "pnpm", []string{"run", "build"}},
		{"testdata/bare", "dev", "npm", []string{"run", "dev"}},
		{"testdata/pm-yarn", "dev", "yarn", []string{"run", "dev"}},
		{"testdata/deno-string", "dev", "deno", []string{"task", "dev"}},
	}

	for _, tc := range tests {
		t.Run(tc.dir, func(t *testing.T) {
			m := mustResolve(t, tc.dir)
			c, err := Parse(m)
			if err != nil {
				t.Fatal(err)
			}
			bin, args := Command(m, c, tc.task)
			if bin != tc.wantBin || !equal(args, tc.wantArgs) {
				t.Errorf("Command = %q %v, want %q %v", bin, args, tc.wantBin, tc.wantArgs)
			}
		})
	}
}

func equal(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
