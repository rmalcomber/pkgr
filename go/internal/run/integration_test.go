package run

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// These tests drive the real package managers. They are the only place that
// covers the Windows-specific part of running a task: npm resolves to npm.cmd
// via PATHEXT, and a batch file needs its arguments escaped under cmd.exe rules
// rather than the usual CreateProcess ones. They skip when the tool is absent
// so the suite still passes on a machine without a JS toolchain.

func skipUnlessInstalled(t *testing.T, bin string) {
	t.Helper()
	if _, err := exec.LookPath(bin); err != nil {
		t.Skipf("%s is not installed", bin)
	}
}

func writeFile(t *testing.T, dir, name, contents string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(contents), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

const integrationPackageJSON = `{
  "name": "pkgr-integration",
  "private": true,
  "scripts": {
    "ok": "node --eval process.exit(0)",
    "fail": "node --eval process.exit(5)",
    "cwd": "node --eval process.exit(require('fs').existsSync('marker')?0:1)"
  }
}`

func TestIntegrationNPM(t *testing.T) {
	skipUnlessInstalled(t, "npm")
	skipUnlessInstalled(t, "node")

	dir := t.TempDir()
	writeFile(t, dir, "package.json", integrationPackageJSON)
	writeFile(t, dir, "marker", "")

	tests := []struct {
		script string
		want   int
	}{
		{"ok", 0},
		{"fail", 5},
		// Proves cmd.Dir reached the script: the marker file only exists in dir.
		{"cwd", 0},
	}

	for _, tc := range tests {
		t.Run(tc.script, func(t *testing.T) {
			code, err := Run(dir, "npm", []string{"run", "--silent", tc.script})
			if err != nil {
				t.Fatalf("Run returned an error: %v", err)
			}
			if code != tc.want {
				t.Errorf("exit code = %d, want %d", code, tc.want)
			}
		})
	}
}

func TestIntegrationDeno(t *testing.T) {
	skipUnlessInstalled(t, "deno")

	dir := t.TempDir()
	// The task bodies are quoted because deno task runs them through its own
	// shell, which would otherwise split on the parentheses.
	writeFile(t, dir, "deno.json", `{
  "tasks": {
    "ok": "deno eval 'Deno.exit(0)'",
    "fail": "deno eval 'Deno.exit(5)'"
  }
}`)

	for _, tc := range []struct {
		task string
		want int
	}{
		{"ok", 0},
		{"fail", 5},
	} {
		t.Run(tc.task, func(t *testing.T) {
			code, err := Run(dir, "deno", []string{"task", "--quiet", tc.task})
			if err != nil {
				t.Fatalf("Run returned an error: %v", err)
			}
			if code != tc.want {
				t.Errorf("exit code = %d, want %d", code, tc.want)
			}
		})
	}
}
