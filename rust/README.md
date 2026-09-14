# pkgr — Rust port

251,392 bytes, one dependency. See the [top-level README](../README.md) for what
pkgr does and how to use it; this covers how this port is built.

## Why it is small

Two deliberate choices, each removing a dependency tree rather than trimming
around one.

### Hand-rolled JSON ([src/json.rs](src/json.rs))

A general-purpose JSON library builds a full document tree, which needs a value
enum covering every type and — to keep scripts in source order — an
order-preserving map. `serde_json` with `preserve_order` pulls in `indexmap`,
`hashbrown`, `equivalent`, `memchr`, `itoa` and `ryu` to do that.

pkgr only wants two things out of a manifest: one named object of tasks, and one
optional string. So the scanner walks the top level, keeps `scripts`/`tasks` and
`packageManager`, and skips every other value without allocating for it. Source
order falls out for free, because tasks are pushed as they are encountered —
there is no map to preserve the order of.

It is a real parser, not a pattern match: string escapes including `\uXXXX`
surrogate pairs, balanced skipping that respects structural bytes inside
strings, and JSONC comments and trailing commas handled inline as whitespace.

### Hand-rolled picker ([src/ui.rs](src/ui.rs))

`dialoguer` brings `console`, `fuzzy-matcher`, `unicode-width` and
`encode_unicode`. The picker here talks to the Win32 console directly:

- `GetConsoleMode` / `SetConsoleMode` to drop line input, echo, and
  `ENABLE_PROCESSED_INPUT` — the last so Ctrl+C arrives as a key rather than
  killing the process.
- `ReadConsoleInputW` for keys, which reports virtual key codes, so arrows
  arrive as events instead of ANSI escape sequences to be parsed back out.
- VT sequences for drawing, available once
  `ENABLE_VIRTUAL_TERMINAL_PROCESSING` is on.
- A `Drop` impl restores the console however the picker exits, panic included.

Filtering is a case-insensitive substring match over name and command, which is
what a script list actually needs.

### What that leaves

```
pkgr
└── windows-sys        Win32 bindings: declarations, not code
```

## Measured

| Variant | Size |
| --- | --- |
| `serde_json` + `dialoguer`, default release profile | 437,248 |
| `serde_json` + `dialoguer`, size profile | 318,976 |
| **hand-rolled, size profile** | **251,392** |

Hand-rolling both saved 67,584 bytes (21%) over the crate-based version at the
same profile. The size profile itself saved 118,272 bytes (27%).

For context, a Rust hello-world built with the same profile is 104,448 bytes,
so pkgr itself accounts for 146,944 of the total. The equivalent figure for the
[Go port](../go/README.md), which now hand-rolls the same two things, is 513,536
on top of a 1,666,560 floor.

## Build

```
cargo build --release
```

The size settings are in `Cargo.toml` under `[profile.release]`:

- `opt-level = "z"` — optimise for size over speed
- `lto = "fat"` — whole-program optimisation across crates
- `codegen-units = 1` — no parallel codegen, so more can be merged away
- `panic = "abort"` — no unwind tables or landing pads
- `strip = true` — no symbols, no debug info

Stable toolchain only; no nightly features. Going further would mean nightly
`build-std` with `panic_immediate_abort` (~50–80 KB) or `#![no_std]` with raw
syscalls (~20–30 KB), both of which cost more in maintenance than the bytes are
worth here.

## Tests

```
cargo test
```

38 tests. The scanner carries the heaviest coverage — order preservation,
object-form Deno tasks, escapes and surrogate pairs, structural bytes inside
strings, comments and trailing commas, and syntax errors carrying an offset.

`ui` and `run` are covered where they are testable without a console: label
rendering, truncation on character boundaries, filtering, and PATH resolution.
The keypress loop and process spawning are not unit-tested.

## Layout

| File | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | CLI, wiring, exit codes |
| [src/manifest.rs](src/manifest.rs) | locating manifests, package-manager detection |
| [src/json.rs](src/json.rs) | the JSONC scanner |
| [src/ui.rs](src/ui.rs) | the picker |
| [src/run.rs](src/run.rs) | PATH resolution and foreground execution |

## Porting to Linux

`src/ui.rs` splits `Terminal` on `#[cfg(windows)]`. The non-Windows stub reports
"not interactive", so everything except the picker already works — a termios
raw-mode implementation of `acquire`, `read_key`, `width` and `clear` is the
whole job. `run.rs` needs no changes: `look_path` already skips PATHEXT off
Windows, and SIGINT reaches the child through the foreground process group.
