# Changelog

Notable changes to pkgr. The release workflow reads the section matching a
tag out of this file and publishes it as that release's notes, so the version
headings and their spelling matter.

## 0.0.1 - 2026-09-15

First official release. pkgr lists the scripts in a `package.json` or the
tasks in a `deno.json`, lets you pick one, and runs it with the right package
manager in the right directory — so you do not have to remember what a project
calls things, or which tool it was set up with.

### Downloads

| File | Platform |
| --- | --- |
| `pkgr-linux-x86_64` | Linux, x86_64 |
| `pkgr-windows-x86_64.exe` | Windows, x86_64 |

Put either on your `PATH` and run `pkgr`. `SHA256SUMS` carries the checksums.

The Linux binary is **statically linked against musl and has no runtime
dependencies at all**, so it runs on any x86_64 Linux — Alpine and older
distributions included — with no glibc version to satisfy. It is 116,840
bytes, around a third of the equivalent dynamically linked build, because it
is compiled with `panic = immediate-abort`: see the caveat below.

### What it does

- Reads `package.json`, `deno.json` and `deno.jsonc`, listing tasks in the
  order the file gives them rather than sorting them. Comments and trailing
  commas are tolerated, which `deno.jsonc` needs and hand-edited files
  sometimes have.
- Picks the package manager from the `packageManager` field if there is one,
  otherwise from whichever lockfile sits next to the manifest
  (`pnpm-lock.yaml`, `yarn.lock`, `bun.lock`/`bun.lockb`,
  `package-lock.json`), falling back to `npm`. Deno manifests run through
  `deno task`.
- Arrow keys, Home, End, Page Up and Page Down move; typing filters on both
  task name and command; Enter runs; Esc or Ctrl+C cancels and exits 130.
- Runs the task in the foreground with stdin, stdout and stderr inherited, so
  colour, progress bars and interactive prompts behave exactly as if you had
  typed the command yourself — and exits with the task's own exit code.
- Ctrl+C while a task runs goes to the task rather than to pkgr, so the task
  shuts down cleanly and still gets to report why.
- With stdin or stdout redirected there is no terminal to draw on, so pkgr
  prints the tasks and the command for each, then exits 1.

### Platforms

Windows and Linux are both tested. macOS shares the unix terminal and should
work, but has not been run, so no binary is published for it yet; `./build.sh`
builds one from source.

### Caveat on the Linux binary

It is built with `panic = immediate-abort`, which is what removes the DWARF
parser, inflate and demangler that Rust's standard library links so a panic
can print a symbolized backtrace — about 170 KB, roughly two thirds of the
binary. The trade is that a panic aborts with no message and no location. pkgr
should not panic, but if it does on Linux you will get a bare `SIGABRT`.
Building from source with `./build.sh` gives a dynamically linked binary that
reports panics normally.
