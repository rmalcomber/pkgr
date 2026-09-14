// Package run executes a task in the foreground, as if the user had typed the
// command themselves.
package run

import (
	"errors"
	"fmt"
	"os"
	"os/exec"
	"os/signal"
)

// Run starts bin with args in dir, wired to the current terminal, waits for it
// to finish and reports its exit code.
//
// Stdin, stdout and stderr are inherited rather than captured, so interactive
// dev servers, progress bars and colour all behave exactly as they would when
// the command is run by hand.
func Run(dir, bin string, args []string) (int, error) {
	// Resolve up front so a missing tool produces a clear message. On Windows
	// LookPath consults PATHEXT, which is what turns "npm" into "npm.cmd".
	path, err := exec.LookPath(bin)
	if err != nil {
		return 0, fmt.Errorf("%s was not found on your PATH", bin)
	}

	cmd := exec.Command(path, args...)
	cmd.Dir = dir
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	// The console delivers Ctrl+C to every process in the group, so the child
	// already receives it. Trapping it here stops pkgr from dying first and
	// lets the child shut down and report its own exit code.
	interrupts := make(chan os.Signal, 1)
	signal.Notify(interrupts, os.Interrupt)
	defer signal.Stop(interrupts)

	if err := cmd.Start(); err != nil {
		return 0, fmt.Errorf("cannot start %s: %w", bin, err)
	}

	err = cmd.Wait()
	if err == nil {
		return 0, nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		return exitErr.ExitCode(), nil
	}
	return 0, fmt.Errorf("%s failed: %w", bin, err)
}
