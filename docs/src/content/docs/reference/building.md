---
title: Building
description: Build scripts, options and tests.
sidebar:
  order: 1
---

```bash
./build.ps1      # Windows
./build.sh       # Linux and macOS
```

Each script gates on `cargo fmt --check`, `clippy` with warnings as errors, and the test suite, then builds with `--locked` and prints the binary size. It also runs `clippy` against the other platform's target when installed.

| `build.ps1` | `build.sh` | Effect |
| --- | --- | --- |
| `-SkipChecks` | `--skip-checks` | build without the fmt, clippy and test gates |
| `-InstallDir <dir>` | `--install-dir <dir>` | copy the binary there, e.g. a directory on your `PATH` |

On Linux you need a C toolchain (`sudo apt install build-essential`). Release optimisations live in `Cargo.toml` under `[profile.release]`, and the script refuses to build if they have been weakened.

## Tests

```bash
cargo test
```

85 tests on Linux and 72 on Windows, with no test-only dependencies.

## Status

Fully working on Windows and Linux. macOS shares the unix terminal code; CI builds and tests it on a macOS runner and release binaries are published from 0.0.2, but the picker has not yet been verified by hand on a Mac.
