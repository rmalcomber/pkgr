---
title: Manifest parsing
description: What pkgr reads from package.json and deno.json, and what it ignores.
sidebar:
  order: 7
---

pkgr does not use a general JSON library. It scans the top level of the manifest once and keeps only what it needs:

| Manifest | Task section | Other field read |
| --- | --- | --- |
| `package.json` | `scripts` | `packageManager` |
| `deno.json`, `deno.jsonc` | `tasks` | none |

Everything else (dependencies, workspaces, compiler options and so on) is skipped without being stored.

## What is supported

- Source order: tasks are listed as they appear.
- Full string escapes, including `\uXXXX` and surrogate pairs.
- `//` and `/* */` comments.
- Trailing commas.
- Structural characters inside strings (`{`, `}`, `,`) do not confuse the scanner.
- Deno object-form tasks (see [Deno tasks](../deno-tasks/)).

## Errors

A syntax error reports the file and the byte offset:

```text
pkgr: ./package.json: expected ':' at byte 214   (example; the message and offset vary)
```

A manifest that parses but has no tasks reports `no scripts or tasks declared`.

## What it does not do

- It does not follow `extends`, workspaces or imports.
- It does not run `pre` and `post` scripts itself. Your package manager does that when it runs the task.
- It does not validate your manifest beyond what it needs to read the tasks.
