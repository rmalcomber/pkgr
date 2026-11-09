---
title: Deno tasks
description: deno.json, deno.jsonc and object-form tasks.
sidebar:
  order: 5
---

pkgr reads the `tasks` section of `deno.json` or `deno.jsonc` and runs the chosen one with `deno task <name>`.

## String tasks

```jsonc
{
  "tasks": {
    "dev": "deno run --watch main.ts",
    "test": "deno test -A"
  }
}
```

Each row shows the task name and its command.

## Object tasks

Deno also allows an object with a `command` and a `description`. pkgr understands both:

```jsonc
{
  "tasks": {
    "build": { "description": "Bundle it", "command": "deno bundle main.ts" }
  }
}
```

The row shows the command followed by the description in brackets. If a task has only a description (for example it only depends on other tasks), the description is shown on its own.

## Comments and trailing commas

Both `deno.json` and `deno.jsonc` may contain comments and trailing commas. pkgr tolerates them in either file, and in `package.json` too.

## Order

Tasks are shown in the order they appear in the file. They are not sorted.
