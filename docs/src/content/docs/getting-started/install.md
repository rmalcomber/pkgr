---
title: Install
description: Download a binary or build from source.
sidebar:
  order: 1
---

Download a binary from the [releases page]({{releases}}) and put it on your `PATH`.

| File | Platform | Notes |
| --- | --- | --- |
| `pkgr-linux-x86_64` | Linux, x86_64 | static musl, runs anywhere including Alpine |
| `pkgr-windows-x86_64.exe` | Windows, x86_64 | |

## Linux

Release assets do not carry a permission bit, so make the file executable and rename it:

```bash
chmod +x pkgr-linux-x86_64
mv pkgr-linux-x86_64 ~/.local/bin/pkgr
```

The rename is what makes the command `pkgr`. The binary does not care what it is called.

## Windows

Drop `pkgr-windows-x86_64.exe` somewhere on your `PATH`, renamed to `pkgr.exe`.

## Verify

`SHA256SUMS` on each release has the checksums: `sha256sum -c SHA256SUMS` on Linux, `Get-FileHash` on Windows.

```bash
pkgr -version
```

## From source

```bash
./build.ps1      # Windows
./build.sh       # Linux and macOS
```

Both run `cargo fmt --check`, `clippy` with warnings as errors, and the tests, then build with `--locked`. The binary lands in `bin/`. See [Building](../../reference/building/) for the options.
