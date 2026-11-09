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
| `pkgr-macos-arm64` | macOS, Apple silicon | not signed or notarised |
| `pkgr-macos-x86_64` | macOS, Intel | not signed or notarised |

## Linux

Release assets do not carry a permission bit, so make the file executable and rename it:

```bash
chmod +x pkgr-linux-x86_64
mv pkgr-linux-x86_64 ~/.local/bin/pkgr
```

The rename is what makes the command `pkgr`. The binary does not care what it is called.

## Windows

Drop `pkgr-windows-x86_64.exe` somewhere on your `PATH`, renamed to `pkgr.exe`.

## macOS

The macOS binaries are not signed or notarised, so macOS quarantines a file downloaded in a browser. Clear the flag, make it executable and rename it:

```bash
xattr -d com.apple.quarantine pkgr-macos-arm64
chmod +x pkgr-macos-arm64
mv pkgr-macos-arm64 /usr/local/bin/pkgr
```

Use `pkgr-macos-x86_64` on an Intel Mac. If macOS still refuses, allow it under **System Settings → Privacy & Security**.

## Verify

`SHA256SUMS` on each release has the checksums: `sha256sum -c SHA256SUMS` on Linux, `shasum -a 256 -c SHA256SUMS` on macOS, `Get-FileHash` on Windows.

```bash
pkgr -version
```

## From source

```bash
./build.ps1      # Windows
./build.sh       # Linux and macOS
```

Both run `cargo fmt --check`, `clippy` with warnings as errors, and the tests, then build with `--locked`. The binary lands in `bin/`. See [Building](../../reference/building/) for the options.
