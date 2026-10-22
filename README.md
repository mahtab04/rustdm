# RustDM (Rust Download Manager)

[![CI](https://github.com/mahtab04/rustdm/actions/workflows/ci.yml/badge.svg)](https://github.com/mahtab04/rustdm/actions/workflows/ci.yml)

An incremental Rust download manager: CLI first, followed by desktop UI, browser
integration, and a web interface. The shared download engine is available as a
library so each interface uses the same behavior.

**Current milestone:** CLI foundation. Parallel HTTP downloads and validated
resume work; queue/history and other CLI features are next.

## Features

- HTTP/HTTPS, certificate verification, redirects, and connection reuse.
- Up to 16 parallel byte-range connections with retries for missing bytes.
- Ctrl+C cancellation and persistent partial files for resumable downloads.
- Live transfer speed and ETA; resumed bytes are excluded from speed calculations.
- Content-Range, length, and validator checks before accepting range data.
- Single-stream fallback, including empty and unknown-length responses.
- Custom request headers for cookies, referrer, and authorization.
- Exclusive writer locking and completion that preserves existing destinations.
- Encapsulated DownloadManager, builder-style options, and a progress observer.

## Repository layout

| Folder | Purpose | Status |
|---|---|---|
| [`crates/core`](crates/core/README.md) | Shared Rust download engine and transfer metrics | Implemented |
| [`apps/cli`](apps/cli/README.md) | `rustdm` CLI and HTTP integration tests | Implemented |
| [`apps/ui`](apps/ui/README.md) | Future desktop UI | Planned |
| [`apps/web`](apps/web/README.md) | Future web dashboard/API | Planned |
| [`extensions/browser`](extensions/browser/README.md) | Future Chrome/Firefox integration | Planned |
| [`docs`](docs/ARCHITECTURE.md) | Architecture and project documentation | Maintained |

This is a Cargo workspace with two current packages: `rustdm-core` and `rustdm`.
All packages share the root Cargo.lock, dependency versions, and target directory.

## Build

Install a current stable Rust toolchain. Windows MSVC builds also require Visual
Studio C++ Build Tools and a Windows SDK.

```sh
git clone https://github.com/mahtab04/rustdm.git
cd rustdm
cargo build --workspace --release --locked
```

On Windows, use a Visual Studio Developer Command Prompt or the included helper:

```powershell
.\build.cmd build --release --locked
.\target\release\rustdm.exe --help
```

The helper discovers installed Visual Studio tools; Cargo must be on PATH.
On Linux, the executable is `./target/release/rustdm`.
To run from source, use `cargo run -p rustdm -- --help`.

## Download and resume

Replace the example URL with a real direct-download URL:

```powershell
.\target\release\rustdm.exe "https://example.com/file.zip" -o "file.zip" -c 8
```

Press Ctrl+C to pause. Continue with the same source and output:

```powershell
.\target\release\rustdm.exe "https://example.com/file.zip" -o "file.zip" --resume
```

Optional request context can be supplied with repeated headers:

```powershell
.\target\release\rustdm.exe "https://example.com/file.zip" -o "file.zip" -H "Referer: https://example.com/page"
```

| Option | Purpose | Default |
|---|---|---|
| `-o, --output` | Destination path | Required |
| `-c, --connections` | Parallel range connections, 1 to 16 | 4 |
| `--resume` | Reuse validated partial files | Off |
| `--retries` | Additional attempts, 0 to 10 | 3 |
| `-H, --header` | Request header; repeatable | None |

New partial state is stored in `<output>.rustdm/`. Resume requires matching source
and resolved URLs, size, and server validator. The server must correctly maintain
its strong ETag or Last-Modified value. Weak ETags alone do not enable resume.
Supply authentication headers again when resuming; headers are not persisted,
although URL query parameters remain part of the saved source URL.

Servers without usable ranges/validators use one stream. Retries restart that
stream from zero; an interrupted non-resumable download currently requires a new
output path. Segments are fixed at startup. Publisher hash verification, dynamic
rebalancing, queues, desktop UI, browser capture, FTP, and media assembly are
future milestones. Existing `.rustidm` partial directories are detected and
resumed in place when there is no `.rustdm` directory for the same output.

Speed is measured over recent progress samples. ETA is an estimate and remains
unknown until a nonzero speed and total file size are available. A stream retry
that resets its byte count also resets the speed measurement.

## Architecture and development

The core uses Rust structs and methods, a ProgressObserver trait, composition,
and ownership. HTTP, saved state, transfers, and orchestration have separate
responsibilities. See [architecture](docs/ARCHITECTURE.md) for the module map and
library example.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

Integration tests use a local HTTP server and known binary data, covering
parallel transfers, pause/resume, truncated responses, changed validators,
invalid ranges, fallback behavior, and destination preservation. CI is configured
for Windows and Linux. Local validation was performed on Windows; the Actions
results show the status of the configured platforms.

- [Roadmap](ROADMAP.md): CLI -> desktop -> browser -> web.
- [Contributing](CONTRIBUTING.md): checks and Conventional Commit workflow.
- [Changelog](CHANGELOG.md): implemented changes and compatibility notes.

This repository contains the new Rust implementation. Build caches, downloaded
files, executable analysis inputs, and extracted vendor code stay outside Git.
