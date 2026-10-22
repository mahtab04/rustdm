# RustIDM

[![CI](https://github.com/mahtab04/rustidm/actions/workflows/ci.yml/badge.svg)](https://github.com/mahtab04/rustidm/actions/workflows/ci.yml)

An incremental Rust download manager: CLI first, followed by desktop UI, browser
integration, and a web interface. The shared download engine is available as a
library so each interface uses the same behavior.

**Current milestone:** CLI foundation. Parallel HTTP downloads and validated
resume work; queue/history and other CLI features are next.

## Features

- HTTP/HTTPS, certificate verification, redirects, and connection reuse.
- Up to 16 parallel byte-range connections with retries for missing bytes.
- Ctrl+C cancellation and persistent partial files for resumable downloads.
- Content-Range, length, and validator checks before accepting range data.
- Single-stream fallback, including empty and unknown-length responses.
- Custom request headers for cookies, referrer, and authorization.
- Exclusive writer locking and completion that preserves existing destinations.
- Encapsulated DownloadManager, builder-style options, and a progress observer.

## Build

Install a current stable Rust toolchain. Windows MSVC builds also require Visual
Studio C++ Build Tools and a Windows SDK.

```sh
git clone https://github.com/mahtab04/rustidm.git
cd rustidm
cargo build --release --locked
```

On Windows, use a Visual Studio Developer Command Prompt or the included helper:

```powershell
.\build.cmd build --release --locked
.\target\release\rustidm.exe --help
```

The helper discovers installed Visual Studio tools; Cargo must be on PATH.
On Linux, the executable is `./target/release/rustidm`.

## Download and resume

Replace the example URL with a real direct-download URL:

```powershell
.\target\release\rustidm.exe "https://example.com/file.zip" -o "file.zip" -c 8
```

Press Ctrl+C to pause. Continue with the same source and output:

```powershell
.\target\release\rustidm.exe "https://example.com/file.zip" -o "file.zip" --resume
```

Optional request context can be supplied with repeated headers:

```powershell
.\target\release\rustidm.exe "https://example.com/file.zip" -o "file.zip" -H "Referer: https://example.com/page"
```

| Option | Purpose | Default |
|---|---|---|
| `-o, --output` | Destination path | Required |
| `-c, --connections` | Parallel range connections, 1 to 16 | 4 |
| `--resume` | Reuse validated partial files | Off |
| `--retries` | Additional attempts, 0 to 10 | 3 |
| `-H, --header` | Request header; repeatable | None |

Partial state is stored in `<output>.rustidm/`. Resume requires matching source
and resolved URLs, size, and server validator. The server must correctly maintain
its strong ETag or Last-Modified value. Weak ETags alone do not enable resume.
Supply authentication headers again when resuming; headers are not persisted,
although URL query parameters remain part of the saved source URL.

Servers without usable ranges/validators use one stream. Retries restart that
stream from zero; an interrupted non-resumable download currently requires a new
output path. Segments are fixed at startup. Publisher hash verification, dynamic
rebalancing, queues, desktop UI, browser capture, FTP, and media assembly are
future milestones. Legacy `.rustdm` partial directories are not migrated.

## Architecture and development

The core uses Rust structs and methods, a ProgressObserver trait, composition,
and ownership. HTTP, saved state, transfers, and orchestration have separate
responsibilities. See [architecture](docs/ARCHITECTURE.md) for the module map and
library example.

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
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
