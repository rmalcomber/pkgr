// Package manifest locates and reads the JS/TS project manifests that pkgr
// knows how to run tasks from: package.json and deno.json[c].
package manifest

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

// Kind identifies which flavour of manifest was found, which in turn decides
// how task names are looked up and which tool runs them.
type Kind int

const (
	KindNPM Kind = iota
	KindDeno
)

func (k Kind) String() string {
	if k == KindDeno {
		return "deno"
	}
	return "npm"
}

// Manifest is a located manifest file on disk, before its contents are read.
type Manifest struct {
	Path string // absolute path to the manifest file
	Dir  string // directory holding it; tasks run with this as the working dir
	Kind Kind
}

// candidates are probed in order when the argument names a directory.
var candidates = []string{"package.json", "deno.json", "deno.jsonc"}

// ErrNotFound reports that a directory held none of the supported manifests.
var ErrNotFound = errors.New("no package.json, deno.json or deno.jsonc found")

// Resolve turns a command-line argument into a located manifest. The argument
// may be empty (meaning the current directory), a directory, or a manifest file
// itself; relative paths are resolved against the current directory.
func Resolve(arg string) (Manifest, error) {
	var m Manifest

	path := arg
	if path == "" {
		wd, err := os.Getwd()
		if err != nil {
			return m, fmt.Errorf("cannot determine current directory: %w", err)
		}
		path = wd
	}

	path, err := filepath.Abs(path)
	if err != nil {
		return m, fmt.Errorf("cannot resolve %q: %w", arg, err)
	}

	info, err := os.Stat(path)
	if err != nil {
		return m, fmt.Errorf("cannot read %s: %w", path, err)
	}

	if info.IsDir() {
		found := ""
		for _, name := range candidates {
			candidate := filepath.Join(path, name)
			if st, err := os.Stat(candidate); err == nil && !st.IsDir() {
				found = candidate
				break
			}
		}
		if found == "" {
			return m, fmt.Errorf("%s: %w", path, ErrNotFound)
		}
		path = found
	}

	kind, err := kindOf(filepath.Base(path))
	if err != nil {
		return m, err
	}

	return Manifest{Path: path, Dir: filepath.Dir(path), Kind: kind}, nil
}

// kindOf maps a manifest file name to its Kind. Unlike the Deno prototype,
// which treated anything that was not package.json as Deno, unrecognised names
// are rejected outright.
func kindOf(base string) (Kind, error) {
	switch base {
	case "package.json":
		return KindNPM, nil
	case "deno.json", "deno.jsonc":
		return KindDeno, nil
	default:
		return 0, fmt.Errorf("%s is not a supported manifest (want package.json, deno.json or deno.jsonc)", base)
	}
}
