---
title: About
description: What pkgr is, who it is for and why it exists.
sidebar:
  order: 1
---

pkgr is a tiny terminal tool that lists the scripts in a `package.json` or the tasks in a `deno.json`, lets you pick one, and runs it with the right package manager in the right directory.

## Why it exists

Every project calls things something different and uses a different tool: `pnpm run dev`, `npm run start`, `deno task serve`. pkgr removes the remembering: type `pkgr`, pick, press Enter.

## Principles

- **Small.** The Linux build is a static binary of about 117 KB. Every byte has to earn its place: see the [Floppy Rule](https://malcomber.com/floppy-rule/) and [Size](/reference/size/).
- **Transparent.** It prints the command it resolved and gets out of the way. The task runs attached to your terminal.
- **No surprises.** Nothing runs if you cancel; the exit code is the task's own.

## Status

The current release is [v{{version}}](/releases/v{{version}}/). Source and issues are on [GitHub]({{repo}}). The story of how it got here is in [History](/project/history/).
