# pkgr — Go port

2,180,096 bytes, zero third-party dependencies. See the
[top-level README](../README.md) for what pkgr does and how to use it; this
covers how this port is built.

## Why it is small

Both the JSON parsing and the picker are hand-rolled, mirroring the
[Rust port](../rust/README.md). `go.mod` has no `require` block at all.

### Hand-rolled JSON ([internal/manifest/json.go](internal/manifest/json.go))

`encoding/json` decodes the whole document, and decoding into a map would lose
the source order that drives the picker — recovering that order means a
streaming decoder walking every value anyway.

pkgr only wants two things out of a manifest: one named object of tasks, and one
optional string. The scanner walks the top level, keeps `scripts`/`tasks` and
`packageManager`, and skips every other value without allocating for it. Source
order falls out for free, because tasks are appended as they are encountered.

It is a real parser: string escapes including `\uXXXX` surrogate pairs, balanced
skipping that respects structural bytes inside strings, and JSONC comments and
trailing commas handled inline as whitespace.

Dropping `encoding/json` also took most of `reflect` with it — 73.7 KB of code
down to 29.4 KB, the remainder being what `fmt` still needs.

### Hand-rolled picker ([internal/ui](internal/ui))

The console API is reached through `syscall.NewLazyDLL`, which is standard
library, so this needs no third-party module:

- `GetConsoleMode` / `SetConsoleMode` to drop line input, echo and
  `ENABLE_PROCESSED_INPUT` — the last so Ctrl+C arrives as a key rather than
  killing the process.
- `ReadConsoleInputW` for keys, which reports virtual key codes, so arrows
  arrive as events instead of ANSI escape sequences to be parsed back out.
- VT sequences for drawing, available once
  `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is on.
- A deferred `release` restores the console however the picker exits, panics
  included.

`INPUT_RECORD` is read straight out of the kernel, so its layout is asserted in
a test — a wrong offset would decode every keypress to nonsense rather than
failing loudly.

### Where the remaining size goes

| Layer | Size | Share |
| --- | --- | --- |
| Go runtime floor (hello world, same flags) | 1,666,560 | 76% |
| pkgr itself | 513,536 | 24% |

Three quarters of the binary is the Go runtime: garbage collector, scheduler,
and the reflection and stack metadata its stdlib needs. That is the floor, and
it cannot be gone below while using Go with its standard library.

The last reducible piece is `fmt`, which is what keeps `reflect` alive. Removing
it in favour of manual string building would save perhaps 190 KB — under 9% —
at the cost of every error message and the picker's rendering becoming harder to
read. Not taken.

## Other implementation details

- **Batch files.** On Windows `exec.LookPath` honours `PATHEXT`, which is what
  resolves `npm` to `npm.cmd`. Go 1.23+ escapes arguments to `.cmd` targets
  under cmd.exe rules automatically, so no manual `cmd.exe /c` wrapper is
  needed — one would only muddy exit codes and Ctrl+C.
- **Ctrl+C** is trapped in the parent with `signal.Notify` so pkgr does not die
  before the child, which then reports its own exit code.
- **No TTY** is detected by `GetConsoleMode` failing on a redirected handle, so
  the picker refuses up front and the task list is printed instead.

## Build

For day-to-day work:

```
go build -o pkgr.exe .
```

For a release build, use the top-level script, which builds both ports:

```
../build.ps1 -Port go -Version 1.0.0
```

That wraps:

```
go build -trimpath -ldflags "-s -w -X main.version=1.0.0" -o pkgr.exe .
```

- `-s -w` drops the symbol table and DWARF debug info.
- `-trimpath` removes local filesystem paths and makes the build reproducible.
- `-X main.version=` stamps the version without editing source.

`CGO_ENABLED=0` is set by the script. It changes nothing on Windows, where Go
binaries are already static, but it matters for the Linux build.

To install into `%GOPATH%\bin`:

```
go install .
```

## Tests

```
go test ./...
```

39 tests. `internal/manifest` covers the scanner directly — ordering, both Deno
task forms, escapes and surrogate pairs, structural bytes inside strings, JSONC,
and syntax errors carrying an offset — plus table tests over `testdata/`
fixtures for resolution and every package-manager detection path.

`internal/run` has integration tests that drive real npm and deno, covering the
Windows-specific part: `.cmd` resolution, argument escaping, exit-code
propagation and the working directory. They skip when the tool is absent, so the
suite still passes without a JS toolchain installed.

`internal/ui` covers label rendering, truncation on rune boundaries, filtering,
frame line-counting, and the `INPUT_RECORD` layout. The keypress loop is not
unit-tested.

## Layout

| Path | Purpose |
| --- | --- |
| [main.go](main.go) | flags, wiring, exit codes |
| [internal/manifest/json.go](internal/manifest/json.go) | the JSONC scanner |
| [internal/manifest](internal/manifest) | locating manifests, package-manager detection |
| [internal/ui/select.go](internal/ui/select.go) | picker logic, platform-independent |
| [internal/ui/console_windows.go](internal/ui/console_windows.go) | Win32 console handling |
| [internal/run/run.go](internal/run/run.go) | foreground execution |

## Porting to Linux

`internal/ui` splits on build tags. `console_other.go` reports "not
interactive", so everything except the picker already works — a termios
raw-mode implementation of `acquireTerminal`, `readKey`, `width` and `clear` is
the whole job. Nothing else needs changing:

```
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -trimpath -ldflags "-s -w" -o pkgr .
```
