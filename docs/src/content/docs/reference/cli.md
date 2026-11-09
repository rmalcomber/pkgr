---
title: Command line
description: Synopsis, flags, output streams and exit codes.
sidebar:
  order: 1
---

```text
pkgr [path]
```

| Argument | Meaning |
| --- | --- |
| `path` | A directory holding `package.json`, `deno.json` or `deno.jsonc`, or one of those files. Defaults to the current directory. |

More than one argument is an error.

## Flags

| Flag | Effect |
| --- | --- |
| `-version`, `--version` | print `pkgr <version>` and exit 0 |
| `-h`, `--help`, `-help` | show usage and exit 0 |

## Output streams

pkgr writes its own messages to **stderr**, so the task's stdout stays clean:

- the resolved command, as `> <tool> run <task>`, just before the task starts
- errors, prefixed with `pkgr:`
- the task list when there is no terminal

## Environment

pkgr reads no environment variables of its own. The task inherits your environment unchanged.

## Exit codes

See [Scripts and CI](../../guides/scripts-and-ci/#exit-codes).
