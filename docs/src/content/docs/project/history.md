---
title: History
description: Three implementations, compared.
sidebar:
  order: 2
---

pkgr started as a Deno prototype, then was ported twice to compare what each language costs for the same tool. Only the Rust port is maintained. The other two are archived under the [`ports-archive`]({{repo}}/tree/ports-archive) tag: `old/` the Deno prototype, `go/` the Go port, and `rust/` the Rust port before it became the repository itself. A tag, not a branch, because nothing builds from there.

| Implementation | Binary (bytes) | Third-party dependencies |
| --- | ---: | ---: |
| Deno prototype (Cliffy) | n/a | 5 |
| Go, hand-rolled | 2,180,096 | 0 |
| **Rust, hand-rolled** | **233,472** | **1** |

The Go port is 9.3 times larger, and nearly all of the gap is the runtime: a Go hello-world is 1,666,560 bytes against Rust's 104,448. Subtract each floor and the actual pkgr code is only 4 times apart.

Go needs no third-party module because it reaches the Win32 console through `syscall.NewLazyDLL` in its standard library. Rust needs `windows-sys` for the same bindings. Go wins on dependency count. Rust wins on size by an order of magnitude.

All three were measured on one machine with one toolchain.
