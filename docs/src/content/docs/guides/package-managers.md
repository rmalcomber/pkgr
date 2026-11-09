---
title: Package managers
description: How pkgr decides between npm, pnpm, yarn, bun and deno.
sidebar:
  order: 4
---

pkgr runs the command **you would have typed**. Choosing the right tool is the whole job, so the rules are short and fixed.

## For package.json

In order:

1. **The `packageManager` field**, if it names a known tool. This is the [Corepack](https://nodejs.org/api/corepack.html) format, e.g. `"packageManager": "pnpm@9.1.0"` gives `pnpm`. Known names are `npm`, `pnpm`, `yarn` and `bun`.
2. **A lockfile next to the manifest**, checked in this order:

   | Lockfile | Tool |
   | --- | --- |
   | `pnpm-lock.yaml` | `pnpm` |
   | `yarn.lock` | `yarn` |
   | `bun.lock` | `bun` |
   | `bun.lockb` | `bun` |
   | `package-lock.json` | `npm` |

3. **`npm`**, as the fallback.

An unrecognised `packageManager` value (say `"foo@1"`) is ignored and detection falls through to the lockfile rather than producing a command that cannot run.

The command is always `<tool> run <task>`.

## For Deno

`deno.json` and `deno.jsonc` always run as:

```bash
deno task <task>
```

Lockfiles are ignored.

## Seeing the decision

pkgr prints the resolved command to stderr just before it runs, so you always see what was chosen:

```text
> pnpm run build
```

If the wrong tool is chosen, set `packageManager` in `package.json`. That is the one source pkgr trusts above all others.
