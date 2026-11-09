---
title: Contributing
description: How to build, test and send a change to pkgr.
sidebar:
  order: 4
---

Contributions are welcome. The repository's `CONTRIBUTING.md` is the source of truth; this is the summary.

## Before you start

pkgr has a deliberate budget: it is a tiny binary with one dependency per platform. A change that adds a dependency or noticeably grows the binary needs a good reason: see the [Floppy Rule](https://malcomber.com/floppy-rule/) and [Size](/reference/size/). For anything bigger than a fix, open an [issue]({{repo}}/issues) first so we can agree the shape.

## Build and test

```bash
./build.sh       # Linux and macOS
./build.ps1      # Windows
```

Each script runs `cargo fmt --check`, `clippy` with warnings as errors, and the tests, then builds with `--locked`. Run those three on their own while you work:

```bash
cargo fmt --check
cargo clippy --release --all-targets --locked -- -D warnings
cargo test --locked
```

CI runs the same gates on Linux, Windows and macOS for every pull request, and builds the release configuration too. See [Building](/reference/building/) and [Testing](/reference/testing/).

## Pull requests

- Branch from `main` and open the pull request against `main`.
- Keep it focused: one change per pull request.
- Add or update tests for behaviour changes.
- Update the docs when behaviour changes (they live in `docs/`), and add a line to `CHANGELOG.md` under the next version.
- Check that the binary size did not grow without a reason.

## Working on the docs

```bash
cd docs
npm install
npm run dev
npm run build     # also fails on broken internal links
```

Pages are Markdown in `docs/src/content/docs/`. Diagrams are Mermaid blocks.

## Releases

Maintainers cut releases: see [Releasing](/reference/releasing/).

## Funding

- [GitHub Sponsors](https://github.com/sponsors/rmalcomber)
- [Buy Me a Coffee](https://buymeacoffee.com/rmalcomber)
