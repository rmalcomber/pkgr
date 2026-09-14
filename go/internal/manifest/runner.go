package manifest

import (
	"os"
	"path/filepath"
	"strings"
)

// lockfiles maps a lockfile name to the package manager that writes it. Order
// matters: the first match wins, so the more specific managers are checked
// before npm's.
var lockfiles = []struct {
	file string
	pm   string
}{
	{"pnpm-lock.yaml", "pnpm"},
	{"yarn.lock", "yarn"},
	{"bun.lock", "bun"},
	{"bun.lockb", "bun"},
	{"package-lock.json", "npm"},
}

// PackageManager reports which tool should run this manifest's tasks. Deno
// manifests always use deno; for package.json the "packageManager" field wins,
// then a lockfile in the same directory, and npm is the fallback.
func PackageManager(m Manifest, c Contents) string {
	if m.Kind == KindDeno {
		return "deno"
	}

	if pm := normalizePackageManager(c.PackageManager); pm != "" {
		return pm
	}

	for _, l := range lockfiles {
		if st, err := os.Stat(filepath.Join(m.Dir, l.file)); err == nil && !st.IsDir() {
			return l.pm
		}
	}

	return "npm"
}

// normalizePackageManager pulls the tool name out of a corepack-style
// "packageManager" value such as "pnpm@9.1.0". Unrecognised names are ignored
// so that a typo falls through to lockfile detection rather than producing a
// command that cannot run.
func normalizePackageManager(field string) string {
	name, _, _ := strings.Cut(strings.TrimSpace(field), "@")
	switch strings.ToLower(name) {
	case "npm", "pnpm", "yarn", "bun":
		return strings.ToLower(name)
	default:
		return ""
	}
}

// Command builds the argv that runs the named task, matching what the user
// would type by hand. "deno task <name>" for Deno; "<pm> run <name>" is valid
// for npm, pnpm, yarn and bun alike.
func Command(m Manifest, c Contents, task string) (bin string, args []string) {
	pm := PackageManager(m, c)
	if pm == "deno" {
		return "deno", []string{"task", task}
	}
	return pm, []string{"run", task}
}
