---
title: FAQ
description: Short answers to common questions.
sidebar:
  order: 3
---

### Why not just use `npm run`?

You can. pkgr is for when you do not remember what a project calls its scripts, or which package manager it uses. It shows the list and runs the right tool.

### Does it change anything in my project?

No. It reads the manifest and starts a process. It writes nothing.

### Does it work with monorepos?

Yes, per package. Point it at a package directory: `pkgr packages/api`. It does not read workspace configuration.

### Does it pass arguments to the task?

No. `pkgr` takes a path only. To pass arguments, run the command it printed yourself.

### Does it send any data anywhere?

No. It has no network code.

### Why is it so small?

On purpose. It hand-rolls the two things it needs (a JSON scanner and a terminal picker) instead of pulling in libraries. See [Size](../../reference/size/) and the [Floppy Rule](https://malcomber.com/floppy-rule/).

### Is there a macOS binary?

Yes, from 0.0.2: `pkgr-macos-arm64` (Apple silicon) and `pkgr-macos-x86_64` (Intel). They are not signed or notarised, so see [Install](/getting-started/install/) for the quarantine step.

### How do I support it?

You can [buy me a coffee](https://buymeacoffee.com/rmalcomber).
