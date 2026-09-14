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

  up/down move · type to filter · enter run · esc cancel
```

Choosing `dev` runs `npm run dev` in that project's directory, attached to your
terminal exactly as if you had typed it yourself.

A single 234 KB binary with one dependency.

## Usage

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

The package manager comes from the `packageManager` field if present
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
cargo build --release
```

The binary lands at `target/release/pkgr.exe`. Put it anywhere on your `PATH`.

Stable toolchain, no nightly features.

## Size

234 KB. Most of that came from two deliberate choices, each removing a dependency
tree rather than trimming around one; the rest came from assumptions about the
input, measured one at a time.

### Hand-rolled JSON ([src/json.rs](src/json.rs))

A general-purpose JSON library builds a full document tree, which needs a value
enum covering every type and — to keep scripts in source order — an
order-preserving map. `serde_json` with `preserve_order` pulls in `indexmap`,
`hashbrown`, `equivalent`, `memchr`, `itoa` and `ryu` to do that.

pkgr only wants two things out of a manifest: one named object of tasks, and one
optional string. So the scanner walks the top level, keeps `scripts`/`tasks` and
`packageManager`, and skips every other value without allocating for it. Source
order falls out for free, because tasks are pushed as they are encountered —
there is no map whose order needs preserving.

It is a real parser, not a pattern match: string escapes including `\uXXXX`
surrogate pairs, balanced skipping that respects structural bytes inside
strings, and JSONC comments and trailing commas handled inline as whitespace.

### Hand-rolled picker ([src/ui.rs](src/ui.rs))

`dialoguer` brings `console`, `fuzzy-matcher`, `unicode-width` and
`encode_unicode`. The picker here talks to the Win32 console directly:

- `GetConsoleMode` / `SetConsoleMode` to drop line input, echo and
  `ENABLE_PROCESSED_INPUT` — the last so Ctrl+C arrives as a key rather than
  killing the process.
- `ReadConsoleInputW` for keys, which reports virtual key codes, so arrows
  arrive as events instead of ANSI escape sequences to be parsed back out.
- VT sequences for drawing, available once
  `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is on.
- A `Drop` impl restores the console however the picker exits, panic included.

Filtering is a case-insensitive substring match over name and command, which is
what a script list actually needs. Case is folded for ASCII only — see below.

### Release profile

In `Cargo.toml` under `[profile.release]`:

| Setting | Effect |
| --- | --- |
| `opt-level = "z"` | optimise for size over speed |
| `lto = "fat"` | whole-program optimisation across crates |
| `codegen-units = 1` | no parallel codegen, so more can be merged away |
| `panic = "abort"` | no unwind tables or landing pads |
| `strip = true` | no symbols, no debug info |

### Measured

| Variant | Size |
| --- | --- |
| `serde_json` + `dialoguer`, default release profile | 437,248 |
| `serde_json` + `dialoguer`, size profile | 318,976 |
| hand-rolled, size profile | 251,904 |
| + ASCII-only case folding | 233,984 |
| **+ `path::absolute` instead of `canonicalize`** | **233,472** |

Hand-rolling both saved 67,072 bytes (21%) over the crate-based version at the
same profile. The size profile itself saved 118,272 bytes (27%).

A Rust hello-world built with the same profile is 104,448 bytes, so pkgr itself
accounts for 129,024 of the total.

### Assumptions, measured

Each candidate was applied alone, rebuilt, and kept only if the binary shrank.

| Assumption | Delta | Kept |
| --- | ---: | --- |
| Script names, PATHEXT and package-manager names are ASCII, so fold case with `to_ascii_lowercase` instead of `to_lowercase` | −17,920 | yes |
| Paths needn't be canonical, so use `std::path::absolute`; this also deletes the `\\?\` stripping `canonicalize` made necessary | −512 | yes |
| Scanner error bytes are ASCII punctuation, so quote them by hand instead of `{:?}` | +512 | no |
| Column padding by hand instead of `{:<16}` / `{:<pad$}` | +512 | no |
| Frame rendering by hand instead of `format!` | 0 | no |
| `opt-level = "s"` instead of `"z"` | +7,168 | no |

The ASCII assumption is the only large one. `to_lowercase` links std's Unicode
case-mapping tables, and nothing else in pkgr needed them. The trade is that
non-ASCII letters no longer fold: typing `étape` does not find `ÉTAPE`, though
`ÉTAPE` does. A test pins that behaviour.

The three formatting experiments show `core::fmt` is effectively a fixed cost:
std's panic handler and `eprintln!`'s own failure path keep it linked, along
with its padding and `char` escaping. Avoiding it at call sites removes nothing.

### Deliberately not done

**Replacing `std::process::Command` with raw `CreateProcessW`.** This is the
largest single item: `Command` adds 84,992 bytes to a minimal binary, so a
hand-rolled spawn could plausibly save tens of KB here. But npm, pnpm and yarn
are `.cmd` shims, and std applies cmd.exe-specific argument escaping — the
CVE-2024-24576 "BatBadBut" fix — precisely because a script name in a cloned
repository's `package.json` such as `x" & calc & "` could otherwise run
arbitrary commands. Re-implementing that escaping correctly is the whole cost
of `Command`, so the saving only materialises by getting it wrong.

Also ruled out: dropping OS error text (std links it anyway via `eprintln!`),
a static CRT (larger), and a platform trait for Linux/macOS (every platform
seam is already a compile-time `#[cfg]`, so it saves nothing).

Going further would mean nightly `build-std` with `panic_immediate_abort`
(~50–80 KB) or `#![no_std]` with raw syscalls (~20–30 KB). Both cost more in
maintenance than the bytes are worth.

### What that leaves

```
pkgr
└── windows-sys        Win32 bindings: declarations, not code
```

## Tests

```
cargo test
```

72 tests, no test-only dependencies.

**The scanner** carries the heaviest coverage — order preservation, object-form
Deno tasks, escapes and surrogate pairs, structural bytes inside strings,
comments and trailing commas, and syntax errors carrying an offset.

**The picker loop** is driven by a scripted console. `run_picker` is generic
over a small `Console` trait — width, write, clear, read_key — so tests feed it
a fixed list of keypresses and read back the frames it drew. That covers
navigation and clamping, the scrolling window, filtering, backspace, and the
redraw bookkeeping: every frame must rewind exactly the line count of the one
before it, or the picker eats scrollback. The trait monomorphises into the real
`Terminal`, so it costs nothing at runtime — the binary is byte-identical with
and without it.

The most valuable of those: the cursor indexes the *filtered* view, so
committing has to map back to the original task index. Getting that wrong runs
the wrong script, silently.

Key decoding is a pure function of the two values the kernel reports, so the
whole mapping — arrows, Home/End, Ctrl+C, printable characters, control
characters — is tested without a console.

**`run`** has integration tests that drive real npm and deno, covering PATH
resolution, exit-code propagation and the working directory. They skip when the
tool is absent, so the suite still passes without a JS toolchain installed.

One of those earns its place: npm ships a `#!/usr/bin/env bash` script named
`npm` right beside `npm.cmd`, and resolving to the extensionless one makes
`CreateProcess` fail with *"%1 is not a valid Win32 application"*. A regression
test builds that exact layout and asserts the launchable file wins.

What remains untested is the Win32 layer itself — `ReadConsoleInputW`,
`SetConsoleMode` and the screen-buffer query — which needs a real console.

## Layout

| Path | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | CLI, wiring, exit codes |
| [src/manifest.rs](src/manifest.rs) | locating manifests, package-manager detection |
| [src/json.rs](src/json.rs) | the JSONC scanner |
| [src/ui.rs](src/ui.rs) | the picker |
| [src/run.rs](src/run.rs) | PATH resolution and foreground execution |

## Status

Windows only so far.

`src/ui.rs` splits `Terminal` on `#[cfg(windows)]`, and the non-Windows stub
reports "not interactive" — so everything except the picker already works on
Linux. A termios raw-mode implementation of `acquire`, `read_key`, `width` and
`clear` is the whole job. `run.rs` needs no changes: `look_path` already skips
PATHEXT off Windows, and SIGINT reaches the child through the foreground
process group.

## History

This started as a Deno prototype, then got ported twice to compare what each
language costs for the same tool. The `historical` branch holds all three side
by side:

| Implementation | Binary | Third-party dependencies |
| --- | --- | --- |
| Deno prototype (Cliffy) | — | 5 |
| Go, hand-rolled | 2,180,096 | 0 |
| **Rust, hand-rolled** | **233,472** | **1** |

The Go port is 9.3× larger, and nearly all of the gap is the runtime: a Go
hello-world is 1,666,560 bytes against Rust's 104,448. Subtract each floor and
the actual pkgr code is only 4× apart. (The Rust figure includes the later
ASCII and `path::absolute` changes, which the Go port on `historical` never
received.)

Go needs no third-party module at all, because it reaches the Win32 console
through `syscall.NewLazyDLL` in its standard library. Rust needs `windows-sys`
for the same bindings. Go wins on dependency count; Rust wins on size by an
order of magnitude.
