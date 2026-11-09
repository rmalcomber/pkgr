---
title: Features
description: Everything pkgr can do, with links to the guide for each.
sidebar:
  order: 2
---

## Finding tasks

- Reads `package.json`, `deno.json` and `deno.jsonc`, listing tasks **in file order**. Comments and trailing commas are tolerated. [Manifest syntax](/guides/manifest-syntax/).
- Choose which project when several are nearby. [Choosing a project](/guides/choosing-a-project/).

## Running them

- Picks the package manager from `packageManager`, then the lockfile, then falls back to `npm`; Deno runs `deno task`. [Package managers](/guides/package-managers/), [Deno tasks](/guides/deno-tasks/).
- Runs in the **foreground** with your terminal attached, and exits with the task's own exit code. [Behaviour](/guides/behaviour/).

## The picker

- Arrow keys, Home, End, Page Up/Down; type to **filter** on name and command; Enter runs; Esc or Ctrl+C cancels (exit 130). [Using pkgr](/guides/using-pkgr/), [Keyboard](/guides/keyboard/).

## Scripts and CI

- With redirected input or output it prints tasks and commands and exits 1. [Scripts and CI](/guides/scripts-and-ci/), [Command line](/reference/cli/).

## Everywhere

- Static Linux binary, Windows `.exe`, and macOS for Apple silicon and Intel. [Install](/getting-started/install/).
