---
title: Size
description: Where the bytes went, and what was deliberately not done.
sidebar:
  order: 2
---

pkgr is a case study in the Floppy Rule. 233 KB on Windows, 117 KB for the published Linux build.

## What made the difference

| Variant | Size (bytes) |
| --- | ---: |
| `serde_json` + `dialoguer`, default release profile | 437,248 |
| `serde_json` + `dialoguer`, size profile | 318,976 |
| hand-rolled JSON and picker, size profile | 251,904 |
| + ASCII-only case folding | 233,984 |
| + `path::absolute` instead of `canonicalize` | 233,472 |

- **Hand-rolled JSON scanner**: pkgr only needs one object of tasks and one optional string, so there is no document tree and no order-preserving map.
- **Hand-rolled picker**: talks to each terminal directly, with a small `Console` trait between the loop and the OS.
- **Release profile**: `opt-level = "z"`, fat LTO, one codegen unit, `panic = "abort"`, stripped.

## Why the Linux build is bigger by default

61% of a normal Linux build is a DWARF parser, a zlib decompressor and a demangler that `std` links so a panic can print a symbolised backtrace. The published Linux binary is built on a pinned nightly with `build-std` and `panic = immediate-abort`, which removes it:

| Build | Size |
| --- | ---: |
| glibc, dynamic | 367,048 |
| musl, static | 459,320 |
| musl + `build-std` | 418,480 |
| **musl + `build-std` + `immediate-abort`** | **116,840** |

The cost is that a crash is a bare `SIGABRT` with no message. `./build.sh` still produces the stable build, which reports panics normally.

## Deliberately not done

Replacing `std::process::Command` with raw `CreateProcessW` would save tens of KB, but npm, pnpm and yarn are `.cmd` shims and `Command` carries the argument escaping that prevents a script name from running arbitrary commands. Re-implementing that correctly costs what it saves.

The full write-up, with every measurement, is in the [README]({{repo}}#size).
