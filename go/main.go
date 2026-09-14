// Command pkgr lists the scripts or tasks declared in a JS/TS project manifest,
// lets you pick one, and runs it in the current terminal.
package main

import (
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"pkgr/internal/manifest"
	"pkgr/internal/run"
	"pkgr/internal/ui"
)

// version is overridable at link time so release builds can stamp a real
// version in: go build -ldflags "-X main.version=1.2.3"
var version = "0.1.0-dev"

// Exit codes beyond the child's own. 130 is the conventional code for a
// command interrupted at the terminal.
const (
	exitUsage     = 1
	exitCancelled = 130
)

func main() {
	os.Exit(pkgr())
}

func pkgr() int {
	showVersion := flag.Bool("version", false, "print the version and exit")
	flag.Usage = usage
	flag.Parse()

	if *showVersion {
		fmt.Println("pkgr", version)
		return 0
	}

	if flag.NArg() > 1 {
		fmt.Fprintf(os.Stderr, "pkgr: expected at most one path, got %d\n", flag.NArg())
		usage()
		return exitUsage
	}

	m, err := manifest.Resolve(flag.Arg(0))
	if err != nil {
		fmt.Fprintln(os.Stderr, "pkgr:", err)
		return exitUsage
	}

	contents, err := manifest.Parse(m)
	if err != nil {
		fmt.Fprintln(os.Stderr, "pkgr:", err)
		return exitUsage
	}

	task, err := ui.SelectTask(title(m), contents.Tasks)
	if err != nil {
		if errors.Is(err, ui.ErrCancelled) {
			return exitCancelled
		}
		fmt.Fprintln(os.Stderr, "pkgr:", err)
		if errors.Is(err, ui.ErrNotInteractive) {
			// Nothing can be picked, but showing what is on offer beats
			// leaving the caller with only an error.
			listTasks(m, contents)
		}
		return exitUsage
	}

	bin, args := manifest.Command(m, contents, task.Name)

	// Echo the resolved command so it is obvious what ran, and so the line can
	// be copied straight back into the shell.
	fmt.Fprintf(os.Stderr, "\n> %s %s\n\n", bin, strings.Join(args, " "))

	code, err := run.Run(m.Dir, bin, args)
	if err != nil {
		fmt.Fprintln(os.Stderr, "pkgr:", err)
		return exitUsage
	}
	return code
}

// title names the manifest being read, shortened to a relative path when it
// sits under the current directory.
func title(m manifest.Manifest) string {
	shown := m.Path
	if wd, err := os.Getwd(); err == nil {
		if rel, err := filepath.Rel(wd, m.Path); err == nil && !strings.HasPrefix(rel, "..") {
			shown = rel
		}
	}
	return fmt.Sprintf("Select a task  (%s)", shown)
}

func usage() {
	fmt.Fprint(os.Stderr, `pkgr - pick and run a script from package.json or deno.json

Usage:
  pkgr [path]

  path   a directory holding package.json, deno.json or deno.jsonc,
         or one of those files directly. Defaults to the current directory.

Flags:
  -version   print the version and exit
  -h         show this help

Examples:
  pkgr
  pkgr .
  pkgr ./packages/api
  pkgr ./packages/api/package.json
`)
}

// listTasks prints the tasks and the command that would run each one, for when
// the picker cannot be shown.
func listTasks(m manifest.Manifest, c manifest.Contents) {
	fmt.Fprintf(os.Stderr, "\nTasks in %s:\n", m.Path)
	for _, t := range c.Tasks {
		bin, args := manifest.Command(m, c, t.Name)
		fmt.Fprintf(os.Stderr, "  %-16s %s %s\n", t.Name, bin, strings.Join(args, " "))
	}
}
