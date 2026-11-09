---
title: Choosing a project
description: Point pkgr at a directory or a specific manifest file.
sidebar:
  order: 3
---

```text
pkgr [path]
```

`path` is optional. It can be a directory or one of the three manifest files directly.

| Invocation | Reads |
| --- | --- |
| `pkgr` | the manifest in the current directory |
| `pkgr .` | the same, spelled out |
| `pkgr ./packages/api` | the manifest in that directory |
| `pkgr ./packages/api/package.json` | that file |
| `pkgr ./deno.jsonc` | that file |

## Which file wins

When you give a directory, pkgr looks for these in order and takes the first it finds:

1. `package.json`
2. `deno.json`
3. `deno.jsonc`

So a directory with both a `package.json` and a `deno.json` is treated as a Node project. To run the Deno tasks there, name the file: `pkgr ./deno.json`.

pkgr does **not** search parent directories. If you run it in a subfolder with no manifest you get an error rather than a surprise project.

## Monorepos

Each package has its own manifest, so point pkgr at the package you want:

```bash
pkgr packages/api
pkgr packages/web
```

The task runs in that package's directory, with that package's package manager detection (see [Package managers](../package-managers/)).

## Where tasks run

A task always runs in the **directory that holds the manifest**, whatever your shell's current directory is. Symlinks are not resolved, so the directory is the one you typed.

## Errors you may see

| Message | Meaning |
| --- | --- |
| `<dir>: no package.json, deno.json or deno.jsonc found` | none of the three files exist there |
| `<name> is not a supported manifest` | you named a file that is not `package.json`, `deno.json` or `deno.jsonc` |
| `cannot read <path>: ...` | the path does not exist or is not readable |
| `no scripts or tasks declared` | the file parsed, but the `scripts` or `tasks` section is missing or empty |
