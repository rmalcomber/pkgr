package manifest

import (
	"errors"
	"fmt"
	"os"
)

// Task is one entry from a manifest's "scripts" (npm) or "tasks" (deno) block.
type Task struct {
	Name        string
	Command     string
	Description string // deno object-form tasks only
}

// Contents is everything pkgr needs out of a manifest, read in a single pass.
type Contents struct {
	Tasks []Task
	// PackageManager is the raw "packageManager" field, e.g. "pnpm@9.1.0".
	// Empty when absent or when the manifest is a Deno one.
	PackageManager string
}

// ErrNoTasks reports that the manifest parsed cleanly but declared no runnable
// entries, so there is nothing to put in the picker.
var ErrNoTasks = errors.New("no scripts or tasks declared")

// Parse reads m and returns its tasks in the order they appear in the file.
func Parse(m Manifest) (Contents, error) {
	raw, err := os.ReadFile(m.Path)
	if err != nil {
		return Contents{}, fmt.Errorf("cannot read %s: %w", m.Path, err)
	}

	taskKey := "scripts"
	if m.Kind == KindDeno {
		taskKey = "tasks"
	}

	c, err := scan(raw, taskKey)
	if err != nil {
		return c, fmt.Errorf("%s: %w", m.Path, err)
	}
	return c, nil
}
