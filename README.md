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

A single binary with one dependency per platform. The published Linux build is
117 KB and statically linked — no glibc, no runtime dependencies at all.

## Install

Download a binary from [the releases
page](https://github.com/rmalcomber/pkgr/releases) and put it on your `PATH`:

| File | Platform | Notes |
| --- | --- | --- |
| `pkgr-linux-x86_64` | Linux, x86_64 | static musl, runs anywhere incl. Alpine |
| `pkgr-windows-x86_64.exe` | Windows, x86_64 | |

`SHA256SUMS` on each release carries the checksums. Or build from source — see
[Build](#build).

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
./build.ps1      # Windows
./build.sh       # Linux and macOS
```

Both scripts do the same thing. Each gates on `cargo fmt --check`, `clippy` with warnings as errors, and the
test suite. Each also runs `clippy` against *the other* platform's target when
it is installed, since the picker is behind a `#[cfg]` and nothing else in the
build would compile it — add it with `rustup target add`, and the check is
skipped with a notice when it is absent. It then builds with `--locked`, so the exact dependency versions in
`Cargo.lock` are used, and prints the binary's size. The binary lands at
`bin/pkgr.exe` (or `bin/pkgr`), which is ignored by git.

| `build.ps1` | `build.sh` | Effect |
| --- | --- | --- |
| `-SkipChecks` | `--skip-checks` | build straight away, without the fmt, clippy and test gates |
| `-InstallDir <dir>` | `--install-dir <dir>` | copy the finished binary there, e.g. a directory on your `PATH` |

These build for the machine they run on. Cargo resolves the OS bindings by
target, so a Windows build never compiles `libc` and a unix build never
compiles `windows-sys` — there is no flag to set and nothing to strip out.

The published Linux binary is built differently, by
[`.github/workflows/release.yml`](.github/workflows/release.yml) rather than by
`build.sh`. Both workflows are described under [Why the Linux binary is
larger](#why-the-linux-binary-is-larger).

On Linux, Rust links through the system C toolchain, so `build.sh` checks for
`cc` up front (`sudo apt install build-essential` on Debian and Ubuntu).

The optimisations themselves live in `Cargo.toml` under `[profile.release]` — see
[Release profile](#release-profile) — so a plain `cargo build --release`
produces the identical binary. The script refuses to build if any of those
settings have been removed or weakened, so a stray edit cannot quietly ship a
larger release.

Building from source needs only a stable toolchain, and that is what
`build.sh` and `build.ps1` use.

The *published* Linux binary is the one exception: it is built on a pinned
nightly so that `build-std` can apply `panic = immediate-abort` to the standard
library, which is what takes it from 459 KB to 117 KB. That is release
packaging, not a source dependency — see [Why the Linux binary is
larger](#why-the-linux-binary-is-larger) for what the setting actually removes
and what it costs.

## Size

233 KB on Windows, 367 KB on Linux. Most of that came from two deliberate
choices, each removing a dependency tree rather than trimming around one; the
rest came from assumptions about the input, measured one at a time.

The 134 KB gap between the two is almost entirely one thing, and it is not
pkgr: Linux `std` statically links a DWARF parser so that a panic can print a
symbolized backtrace. See [Why the Linux binary is
larger](#why-the-linux-binary-is-larger).

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

### Hand-rolled picker ([src/ui/](src/ui/))

`dialoguer` brings `console`, `fuzzy-matcher`, `unicode-width` and
`encode_unicode`. The picker here talks to each platform's terminal directly.
The loop, its rendering and its filtering sit in
[src/ui/mod.rs](src/ui/mod.rs) and know nothing about the operating system;
they reach a terminal only through the `Console` trait, so a port is a new file
rather than a new branch in the loop.

On Windows ([src/ui/windows.rs](src/ui/windows.rs)):

- `GetConsoleMode` / `SetConsoleMode` to drop line input, echo and
  `ENABLE_PROCESSED_INPUT` — the last so Ctrl+C arrives as a key rather than
  killing the process.
- `ReadConsoleInputW` for keys, which reports virtual key codes, so arrows
  arrive as events instead of ANSI escape sequences to be parsed back out.
- VT sequences for drawing, available once
  `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is on.
- A `Drop` impl restores the console however the picker exits, panic included.

On unix ([src/ui/unix.rs](src/ui/unix.rs)) the same job takes more work,
because a terminal delivers bytes rather than decoded keys:

- `tcgetattr` / `tcsetattr` to clear `ICANON`, `ECHO` and `ISIG` — the last so
  Ctrl+C arrives as a byte rather than killing the process, which is what
  dropping `ENABLE_PROCESSED_INPUT` buys on Windows. `OPOST` deliberately stays
  on: frames end their lines with a bare `\n`, which without it would step down
  a row without returning to column 0 and shear the list into a diagonal.
- `TIOCGWINSZ` for the width, where Windows has
  `GetConsoleScreenBufferInfo`.
- An escape-sequence decoder, since arrows arrive as `ESC [ A` rather than as
  key codes. It reads to a sequence's final byte whether or not it recognises
  it, so an unhandled one — a mouse report, a bracketed-paste marker — is
  swallowed whole instead of leaking its tail into the filter, and it
  reassembles UTF-8 so a multi-byte character typed into the filter arrives as
  one `char`.
- A bare Esc is only distinguishable from the start of a sequence by waiting,
  so `poll` gives the rest of a sequence 50 ms to arrive. Bytes are read from
  the file descriptor rather than through `io::stdin`, whose buffering would
  swallow those bytes where `poll` could no longer see them.

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

On Linux:

| Variant | Size |
| --- | --- |
| hello-world floor, same profile | 287,848 |
| `libc` declared but unused | 287,848 |
| **pkgr** | **367,048** |

The middle row is the one that matters for the split: adding `libc` to
`Cargo.toml` and building without calling into it moved the binary by **0
bytes**, which is what "declarations, not code" has to mean to be worth
claiming. The Windows binary is unchanged at 233,472, and cannot be otherwise —
`libc` is never in its build graph.

### Why the Linux binary is larger

Not the C library, and not the port. Building unstripped and summing the symbol
table by originating crate:

```
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release
nm --size-sort -S --demangle target/release/pkgr
```


| | Bytes | Share |
| --- | ---: | ---: |
| `gimli` + `addr2line` — DWARF debug-info parser | 136,388 | 48.9% |
| everything else (std, core, alloc) | 56,285 | 20.2% |
| **pkgr's own symbols** | **34,094** | **12.2%** |
| `rustc_demangle` | 20,489 | 7.3% |
| `core::fmt` | 15,073 | 5.4% |
| `miniz_oxide` — zlib inflate | 9,570 | 3.4% |
| `backtrace_rs` glue | 4,057 | 1.5% |
| panic machinery | 3,044 | 1.1% |

**170,581 bytes — 61% of the code — exists to symbolize a panic backtrace.**
Linux `std` links a complete DWARF reader to map addresses back to
file-and-line, a zlib decompressor because `.debug_*` sections may be
compressed, and a demangler because Rust symbols need unmangling. None of it
runs unless pkgr crashes.

Windows links none of it: `std` symbolizes through `dbghelp.dll`, which ships
with the OS. Same feature, nothing in the binary.

Two independent measurements agree on this. The hello-world floors differ by
183,400 bytes (287,848 against 104,448), and the backtrace machinery measured
directly in the symbol table is 170,581 — 93% of the gap. Subtract it and Linux
would land near 196 KB, *below* the Windows binary, which is consistent with
the other half of the picture: pkgr's own code is leaner on Linux, since the
Windows build also carries `PATHEXT` resolution and std's UTF-16 path
conversions.

The two ways of attributing pkgr's own cost measure different things and are
both worth keeping in mind: subtracting the hello-world floor gives 79,200
bytes — everything pkgr adds, including the std it reaches for that a
hello-world never does, such as `Command` and the filesystem — while the symbol
table attributes 34,094 bytes to pkgr's own code alone.

`panic = "abort"` does not help on its own: the abort path still prints a
message and a backtrace first, so the parser stays linked. Removing it means
rebuilding the standard library itself, which is a nightly feature —
`build-std` with `panic = immediate-abort`. Every Linux build, measured:

| Build | Size | Toolchain | Runtime deps | Panic message |
| --- | ---: | --- | --- | --- |
| glibc, dynamic (`./build.sh`) | 367,048 | stable | glibc ≥ 2.35 | yes |
| musl, static | 459,320 | stable | none | yes |
| musl + `build-std` | 418,480 | nightly | none | yes |
| **musl + `build-std` + `immediate-abort`** | **116,840** | nightly | none | **no** |

The last row is what the releases page ships. It is a third of the glibc build
and half the Windows one, with no libc to satisfy at runtime. Note that
`build-std` alone accounts for only 41 KB of that: nearly all of the win is
`immediate-abort` discarding the backtrace machinery and the panic formatting
that reaches it.

What it costs is panic reporting. A panic aborts with no message and no
location, so a crash on Linux is a bare `SIGABRT`. `./build.sh` still produces
the dynamically linked stable build, which reports panics normally, and that is
what to reach for when debugging.

The pin is the other cost, and it is not theoretical: the flag this depends on
was renamed from `-Z build-std-features=panic_immediate_abort` to
`-Zunstable-options -Cpanic=immediate-abort`, so the documented invocation now
fails outright. Both workflows name the same dated nightly, and CI builds this
target on every push, so a broken pin surfaces on the commit that breaks it
rather than on the tag that needed it.

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
a static CRT (larger), and a *runtime* platform trait (every platform seam is
a compile-time `#[cfg]` selecting one of the two `Terminal` types, so dispatch
would add code rather than remove it).

This section used to rule out nightly `build-std` on an estimate of ~50–80 KB.
That estimate was wrong by a factor of three, so the released Linux binary now
uses it — measured below. `#![no_std]` with raw syscalls (~20–30 KB more) is
still ruled out: it would mean reimplementing `Command`, which is the one thing
[Deliberately not done](#deliberately-not-done) already argues against.

### What that leaves

```
pkgr
├── windows-sys        Win32 bindings   (windows targets only)
└── libc               libc bindings    (unix targets only)
```

Never both. Each is declared under a `[target.'cfg(...)'.dependencies]` key, so
Cargo leaves the other out of the build graph entirely rather than compiling it
and letting the linker drop it — `cargo tree --target x86_64-pc-windows-msvc`
shows no `libc`, and the Linux tree shows no `windows-sys`. Both crates are
extern declarations, type aliases and constants rather than code, so neither
costs anything at link time either way.

## Tests

```
cargo test
```

85 tests on Linux and 72 on Windows, no test-only dependencies. The counts
differ because each platform's terminal, `look_in` and path handling are tested
against that platform only; 66 of the tests are shared.

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

**Key decoding** is a pure function of what arrives from the OS on each
platform, so both mappings are tested without a terminal. On Windows that is
the two values the kernel reports. On unix the decoder reads through a `Bytes`
trait rather than from stdin, so tests drive it from a fixed byte queue: every
spelling of Home and End, CSI and SS3 arrows, modified arrows, page keys, a
bare Esc, truncated sequences, and multi-byte UTF-8.

Two of those are worth naming, because both failures are silent rather than
loud. An unrecognised sequence must be consumed through to its final byte, or
its tail decodes as keystrokes and lands in the filter — the test asserts that
the key after a mouse report is the key the user actually pressed. And a
multi-byte character must be reassembled before it reaches the filter, or
typing `é` pushes two replacement characters into it.

**`run`** has integration tests that drive real npm and deno, covering PATH
resolution, exit-code propagation and the working directory. They skip when the
tool is absent, so the suite still passes without a JS toolchain installed.
npm hands each script body to a shell — `cmd.exe` on Windows, `sh` elsewhere —
so the fixtures are spelled in the intersection of the two: no parentheses,
which are bare syntax to one and a syntax error to the other, and no quotes,
which one strips and the other passes through.

Each platform's `look_in` is tested against the arrangement that actually
breaks it: on Windows, npm's extensionless shim sitting next to `npm.cmd`; on
unix, a file without the execute bit shadowing the real tool earlier on `PATH`.

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
| [src/ui/](src/ui/) | the picker: `mod.rs` the loop, `windows.rs` and `unix.rs` the two terminals |
| [src/run.rs](src/run.rs) | PATH resolution and foreground execution |

## Status

Fully working on Windows and Linux.

macOS shares the unix terminal and should work, but has not been run. Nothing
in `src/ui/unix.rs` is Linux-specific — `libc` carries the `termios` layout and
`ioctl` signature differences — so the gap is verification, not code.

Each platform is checked against the other's target with `clippy -D warnings`
as part of its build script, since the picker is behind a `#[cfg]` and nothing
else in the build would compile it.

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
