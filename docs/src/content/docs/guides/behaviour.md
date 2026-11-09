---
title: How tasks run
description: Package manager detection, exit codes and signals.
sidebar:
  order: 2
---

## Which tool runs the task

| Manifest | Command |
| --- | --- |
| `deno.json`, `deno.jsonc` | `deno task <name>` |
| `package.json` | `<package manager> run <name>` |

The package manager comes from the `packageManager` field if present (`"pnpm@9.1.0"` gives `pnpm`). Otherwise pkgr looks at the lockfile beside the manifest: `pnpm-lock.yaml`, `yarn.lock`, `bun.lock` or `bun.lockb`, `package-lock.json`. It falls back to `npm`.

## Behaviour worth knowing

- The task runs in the **foreground**, inheriting stdin, stdout and stderr, so colour, progress bars and interactive prompts all work.
- pkgr exits with the **task's own exit code**.
- <kbd>Ctrl</kbd>+<kbd>C</kbd> goes to the task, not to pkgr, so the task shuts down cleanly.
- Tasks are listed in the order they appear in the file, not alphabetically.
- Comments and trailing commas are tolerated in any manifest.
- Cancelling at the picker exits with code 130 and runs nothing.
