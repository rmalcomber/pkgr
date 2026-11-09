---
title: pkgr
description: Pick a script from package.json or a task from deno.json and run it. One tiny binary.
template: splash
hero:
  tagline: Pick a script from package.json or a task from deno.json and run it, without remembering what the project calls things.
  actions:
    - text: Get started
      link: getting-started/install/
      icon: right-arrow
    - text: View on GitHub
      link: '{{repo}}'
      variant: minimal
---

![pkgr showing a list of tasks](../../assets/screenshots/picker.svg)

Choosing `dev` runs `pnpm run dev` (or `npm`, `yarn`, `bun`, `deno task`, whichever the project uses) in that project's directory, attached to your terminal exactly as if you had typed it yourself.

- **Tiny**: the published Linux build is 117 KB, statically linked, with no runtime dependencies. Windows is about 233 KB.
- **Honest**: runs in the foreground, passes through colour and prompts, and exits with the task's own exit code.
- **Predictable**: tasks are listed in file order, and nothing runs if you cancel.

Part of the [Floppy Rule](https://malcomber.com/floppy-rule/): every byte has to earn its place.

If pkgr saves you time, you can [buy me a coffee](https://buymeacoffee.com/rmalcomber).
