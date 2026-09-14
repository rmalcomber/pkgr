# pkgr

Pick a script from `package.json` or a task from `deno.json` and run it, without
having to remember what the project calls things.

```
> pkgr

  Select a task  (package.json)

  > dev      vite
    build    tsc -p . && vite build
    test     vitest run
    lint     eslint .
```

Choosing `dev` runs `npm run dev` in that project's directory, attached to your
terminal exactly as if you had typed it yourself.

The same tool is implemented twice, in Go and in Rust, to see what each costs.
Both ports are complete and behave identically.

Both ports hand-roll the same two things — the JSON parsing and the picker — so
this is as close to a like-for-like comparison as it gets.

| | [Go](go/) | [Rust](rust/) |
| --- | --- | --- |
| Binary size | 2,180,096 bytes (2.08 MB) | **251,392 bytes (0.24 MB)** |
| Third-party dependencies | **0** | 1 |
| Runtime floor (hello world, same flags) | 1,666,560 | 104,448 |
| Above that floor | 513,536 | 146,944 |
| Implementation lines | 1,322 | 1,183 |
| Test lines | 812 | 409 |
| Tests | 39 | 38 |
| JSON parsing | hand-rolled scanner | hand-rolled scanner |
| Picker | hand-rolled Win32 console | hand-rolled Win32 console |

**The Rust build is 8.7× smaller, and almost all of the gap is the runtime.**
Go ships a garbage collector, a scheduler, and the reflection and stack metadata
its stdlib needs — 1.59 MB before any of your code exists. Strip that out and
the two ports are far closer: 514 KB of actual pkgr in Go against 147 KB in
Rust, a 3.5× difference rather than 8.7×.

Go is now within 514 KB of a floor it cannot go below. Rust's floor is 15×
lower, which is the whole story in one number.

The equivalent-code comparison is fair, but note what it cost: `go.mod` has no
`require` block at all, because Go reaches the Win32 console through
`syscall.NewLazyDLL` in the standard library. Rust still needs `windows-sys` for
the same bindings. Go wins on dependency count; Rust wins on size by an order of
magnitude.

### How the Go port got there

| Stage | Size | Change |
| --- | --- | --- |
| `charmbracelet/huh` + `encoding/json` | 4,857,344 | — |
| Hand-rolled picker | 2,547,200 | −2,310,144 |
| Hand-rolled JSON scanner | **2,180,096** | −367,104 |

Dropping the TUI stack removed 24 third-party modules and 47% of the binary.
Dropping `encoding/json` removed a further 367 KB, and took most of `reflect`
with it — from 73.7 KB of code down to 29.4 KB, the remainder being what `fmt`
still needs.

## Usage

Identical for both binaries.

```
pkgr [path]
```

| Invocation | Reads |
| --- | --- |
| `pkgr` | the manifest in the current directory |
| `pkgr .` | the same, spelled out |
| `pkgr ./packages/api` | the manifest in that directory |
| `pkgr ./packages/api/package.json` | that file |

When given a directory, `pkgr` looks for `package.json`, then `deno.json`, then
`deno.jsonc`. It does not search parent directories.

Flags: `-version`, `-h`.

In the picker, arrow keys move, typing filters, Enter runs, and Esc or Ctrl+C
cancels without running anything.

### Which tool runs the task

| Manifest | Command |
| --- | --- |
| `deno.json`, `deno.jsonc` | `deno task <name>` |
| `package.json` | `<package manager> run <name>` |

The package manager is taken from the `packageManager` field if present
(`"pnpm@9.1.0"` → `pnpm`), otherwise from whichever lockfile sits next to the
manifest — `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`/`bun.lockb`,
`package-lock.json` — falling back to `npm`.

### Behaviour worth knowing

- The task runs in the **foreground**, inheriting stdin, stdout and stderr, so
  colour, progress bars and interactive prompts all work.
- `pkgr` exits with the **task's own exit code**.
- Ctrl+C goes to the task, not to `pkgr`, so the task shuts down cleanly.
- Tasks are listed in the order they appear in the file, not alphabetically.
- Comments and trailing commas are tolerated in any manifest, which `deno.jsonc`
  needs and hand-edited files sometimes have.
- Cancelling at the picker exits with code 130 and runs nothing.
- With stdin or stdout redirected there is no terminal to draw on, so `pkgr`
  prints the available tasks and the command for each, then exits 1.

## Build

```
./build.ps1
```

Builds both ports into `bin/` and prints their sizes. `-Port go` or
`-Port rust` builds just one; `-Version 1.0.0` stamps a version.

Per-port detail, including the size flags and how each is tested, is in
[go/README.md](go/README.md) and [rust/README.md](rust/README.md).

## Layout

| Path | Purpose |
| --- | --- |
| [go/](go/) | the Go port |
| [rust/](rust/) | the Rust port |
| [old/](old/) | the original Deno prototype this started as, kept for reference |
| `build.ps1` | builds either or both |

## Status

Windows first, as intended. Neither port has been built or run on Linux yet.

The Go port should cross-compile unchanged. The Rust port will need a termios
raw-mode implementation for its picker: `rust/src/ui.rs` has a `#[cfg(windows)]`
`Terminal` and a non-Windows stub that currently reports "not interactive", so
everything except the picker already works there.
