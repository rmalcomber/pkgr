---
title: Testing
description: How pkgr is tested and what is not.
sidebar:
  order: 4
---

```bash
cargo test
```

85 tests on Linux and 72 on Windows, with no test-only dependencies. 66 are shared. The rest are platform-specific terminal, path-lookup and key-decoding tests.

## The scanner

Heaviest coverage: order preservation, object-form Deno tasks, escapes and surrogate pairs, structural bytes inside strings, comments, trailing commas, and syntax errors carrying an offset.

## The picker loop

Driven by a scripted console. Tests feed it a fixed list of keypresses and read back the frames it drew. That covers navigation and clamping, the scrolling window, filtering, backspace, and redraw bookkeeping: every frame must rewind exactly the line count of the one before, or the picker would eat scrollback.

The most valuable test: the cursor indexes the *filtered* view, so committing must map back to the original index. Getting that wrong would run the wrong script, silently.

## Key decoding

A pure function of what arrives from the OS, so both mappings are tested without a terminal. Cases include every spelling of Home and End, arrows, modified arrows, page keys, a bare Esc, truncated sequences and multi-byte UTF-8. An unrecognised sequence must be consumed through its final byte, or its tail would land in the filter.

## Running real tools

`run` has integration tests that drive real npm and deno, covering `PATH` resolution, exit-code propagation and the working directory. They skip when the tool is missing, so the suite passes without a JavaScript toolchain.

## Not tested

The Win32 layer itself (`ReadConsoleInputW`, `SetConsoleMode`, the screen-buffer query) needs a real console.
