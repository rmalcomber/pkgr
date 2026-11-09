---
title: Troubleshooting
description: Common problems and what to check.
sidebar:
  order: 9
---

## `no package.json, deno.json or deno.jsonc found`

pkgr looks only in the directory you give it (or the current one). It never searches parent directories. Change into the project, or pass the path: `pkgr ./path/to/project`.

## `pkgr needs an interactive terminal to choose a task`

stdin or stdout is redirected. Run pkgr directly in a terminal, not through a pipe or a tool that captures output. The tasks are still printed so you can see what is available.

## It picked the wrong package manager

pkgr trusts, in order: the `packageManager` field, the lockfile beside the manifest, then falls back to `npm`. Set `packageManager` in `package.json` (for example `"pnpm@9.1.0"`). See [Package managers](../package-managers/).

If two lockfiles exist, the first in this order wins: `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`, `bun.lockb`, `package-lock.json`.

## `no scripts or tasks declared`

The file parsed, but there is no `scripts` object (`package.json`) or `tasks` object (`deno.json`), or it is empty.

## A syntax error with a byte offset

Open the manifest and go to that byte position. Comments and trailing commas are fine, so look for a missing quote, colon or brace.

## The Linux binary crashed with no message

The published Linux binary is built with `panic = immediate-abort` to keep it at about 117 KB, so a crash is a bare `SIGABRT` with no message. Build from source with `./build.sh` to get a binary that reports panics, and please [open an issue]({{repo}}/issues) with what you ran.

## On Linux the file will not run

Release downloads have no execute permission:

```bash
chmod +x pkgr-linux-x86_64
```

## On Windows, `npm` fails with "%1 is not a valid Win32 application"

pkgr is built to avoid this: it resolves `npm.cmd` rather than the extensionless shell script that npm ships beside it. If you see it, your `PATH` probably has an unusual layout. Please open an issue.

## The terminal looks sheared or scrollback is eaten

This should not happen: every redraw rewinds exactly the number of lines it painted. If it does, report your terminal and OS.

## macOS

The macOS binaries are not signed or notarised. If you see "cannot be opened because the developer cannot be verified", clear the quarantine flag with `xattr -d com.apple.quarantine pkgr-macos-arm64` (or allow it under System Settings → Privacy & Security). CI builds and tests the unix terminal code on macOS, but the picker has not yet been verified by hand in Terminal.app or iTerm: if something looks wrong, please open an issue with your terminal and macOS version.
