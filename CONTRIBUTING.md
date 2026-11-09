# Contributing to pkgr

Contributions are welcome. Full guidance is on the docs site: https://rmalcomber.github.io/pkgr/project/contributing/

- pkgr has a size budget (one dependency per platform, a tiny binary). Open an issue before anything larger than a fix.
- `./build.sh` (Linux, macOS) or `./build.ps1` (Windows) runs `cargo fmt --check`, `clippy -D warnings` and the tests.
- Branch from `main`, open the pull request against `main`, keep it focused, add tests, update the docs and `CHANGELOG.md`.

## Funding

- GitHub Sponsors: https://github.com/sponsors/rmalcomber
- Buy Me a Coffee: https://buymeacoffee.com/rmalcomber
