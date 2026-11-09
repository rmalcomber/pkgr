---
title: Changelog
description: Release notes.
sidebar:
  order: 1
---

## 0.0.1 (2026-09-15)

First official release. pkgr lists the scripts in a `package.json` or the tasks in a `deno.json`, lets you pick one, and runs it with the right package manager in the right directory.

### Downloads

| File | Platform |
| --- | --- |
| `pkgr-linux-x86_64` | Linux, x86_64 |
| `pkgr-windows-x86_64.exe` | Windows, x86_64 |

The Linux binary is statically linked against musl with no runtime dependencies. It is 116,840 bytes.

### What it does

- Reads `package.json`, `deno.json` and `deno.jsonc`, listing tasks in file order. Comments and trailing commas are tolerated.
- Picks the package manager from `packageManager`, then the lockfile, then falls back to `npm`. Deno runs through `deno task`.
- Arrow keys, Home, End, Page Up and Page Down move. Typing filters on name and command. Enter runs. Esc or Ctrl+C cancels with exit code 130.
- Runs the task in the foreground with stdin, stdout and stderr inherited, and exits with the task's exit code.
- Ctrl+C during a task goes to the task.
- With redirected stdin or stdout it prints the tasks and exits 1.

### Caveat on the Linux binary

It is built with `panic = immediate-abort`, which removes about 170 KB of backtrace machinery. A panic aborts with no message. `./build.sh` gives a binary that reports panics normally.

Full notes: [releases]({{releases}}).
