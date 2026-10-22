# RustDM (Rust Download Manager)

[![CI](https://github.com/mahtab04/rustdm/actions/workflows/ci.yml/badge.svg)](https://github.com/mahtab04/rustdm/actions/workflows/ci.yml)

An incremental Rust download manager: CLI first, followed by desktop UI, browser
integration, and a web interface. The shared download engine is available as a
library so each interface uses the same behavior.

**Current milestone:** desktop UI. The v0.1.0 CLI is released. A native desktop
preview now provides a modern dashboard, download dialog, live progress,
queue controls, and history using the same Rust engine.

## Features

- HTTP/HTTPS, certificate verification, redirects, and connection reuse.
- Up to 16 parallel byte-range connections with retries for missing bytes.
- Dynamic dispatch of saved ranges, so free connections pick up pending work.
- Ctrl+C cancellation and persistent partial files for resumable downloads.
- Live transfer speed and ETA; resumed bytes are excluded from speed calculations.
- Content-Range, length, and validator checks before accepting range data.
- Single-stream fallback, including empty and unknown-length responses.
- Custom request headers for cookies, referrer, and authorization.
- Exclusive writer locking and completion that preserves existing destinations.
- Encapsulated DownloadManager, builder-style options, and a progress observer.
- Persistent queue with sequential execution, retry, and per-attempt history.
- Per-download bandwidth caps shared across parallel connections and retries.
- Optional SHA-256 verification before publishing the final file.

## Repository layout

| Folder | Purpose | Status |
|---|---|---|
| [`crates/core`](crates/core/README.md) | Shared Rust download engine and transfer metrics | Implemented |
| [`apps/cli`](apps/cli/README.md) | `rustdm` command-line interface | Implemented |
| [`apps/ui`](apps/ui/README.md) | Native desktop dashboard and download dialog | Preview |
| [`apps/web`](apps/web/README.md) | Future web dashboard/API | Planned |
| [`extensions/browser`](extensions/browser/README.md) | Future Chrome/Firefox integration | Planned |
| [`tests`](tests/README.md) | Centralized core, CLI, and future interface tests | Maintained |
| [`docs`](docs/ARCHITECTURE.md) | Architecture and project documentation | Maintained |

This is a Cargo workspace with three packages: `rustdm-core`, `rustdm`, and `rustdm-ui`.
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

CI also packages portable Windows/Linux builds with SHA-256 checksum files.
See [release packaging and checklist](docs/RELEASE.md) for artifact verification
and the draft-release workflow. Download the published
[v0.1.0 CLI release](https://github.com/mahtab04/rustdm/releases/tag/v0.1.0)
for Windows x86-64 or Linux x86-64.

## Desktop preview

```powershell
.\build.cmd build -p rustdm-ui --release --locked
.\target\release\rustdm-ui.exe
```

Click **New download**, paste a URL, choose a destination filename, and select
**Download now**. Pause/resume the queue, retry failed jobs, and inspect history
from the dashboard. Transfer options include connection count, speed limit, and
SHA-256 verification. See [desktop usage and storage](apps/ui/README.md).
The published v0.1.0 downloads contain the CLI; desktop packaging comes later.

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
| `--limit-rate` | Aggregate body bytes per second, positive integer | Unlimited |
| `--sha256` | Expected digest: 64 hexadecimal characters | No checksum verification |
| `-H, --header` | Request header; repeatable | None |

New partial state is stored in `<output>.rustdm/`. Resume requires matching source
and resolved URLs, size, and server validator. The server must correctly maintain
its strong ETag or Last-Modified value. Weak ETags alone do not enable resume.
Supply authentication headers again when resuming; headers are not persisted,
although URL query parameters remain part of the saved source URL.

Servers without usable ranges/validators use one stream. Retries restart that
stream from zero; an interrupted non-resumable download currently requires a new
output path. Large files use multiple saved ranges per connection; free
connections claim pending ranges as they finish. Range boundaries stay fixed for
safe resume. Splitting an already active slow range, desktop UI, browser capture,
FTP, and media assembly remain future work. Existing `.rustidm` partial directories are detected and
resumed in place when there is no `.rustdm` directory for the same output.

`--connections` bounds active transfer workers, including when resuming a download
created with a different connection count. Large downloads plan up to eight
ranges per connection (at most 128 saved parts), with a 256 KiB planning threshold
to avoid excessive small requests. Each connection processes pending ranges in
order as it becomes free. Saved range indices remain stable across retries and
resumes. Older partial downloads with up to 16 parts remain supported; older
RustDM builds cannot resume new manifests containing more than 16 parts.

Speed is measured over recent progress samples. ETA is an estimate and remains
unknown until a nonzero speed and total file size are available. A stream retry
that resets its byte count also resets the speed measurement.

## Bandwidth limits

```sh
rustdm download "https://example.com/file.zip" -o file.zip -c 8 --limit-rate 1048576
rustdm queue add "https://example.com/file.zip" -o queued.zip --limit-rate 1048576
```

`--limit-rate 1048576` caps a download at 1 MiB/s (1,048,576 bytes per second)
across all its connections. Supply a positive integer in bytes per second; omit
it for unlimited speed. Suffixes such as `1MiB` and zero are rejected. Legacy
`rustdm URL -o FILE --limit-rate N` syntax also works.

The engine paces response-body consumption in small slices, including fallback
and unknown-length streams. Retries share the same limiter. Saved resume bytes
are not charged again, and Ctrl+C interrupts throttled waits. Network/TLS headers,
probe traffic, and buffering in HTTP/TCP layers are outside this budget; short
network bursts can occur and actual throughput can be lower than the cap.
This is a per-download limit; separate RustDM processes have independent budgets.
Direct resumes may choose a different rate by supplying the option again.
Queued jobs persist their rate for later attempts; older records default to
unlimited. Changing a running job's rate is a future feature.

## Checksum verification

Supply the publisher's expected SHA-256 digest with `--sha256`:

```powershell
.\target\release\rustdm.exe download "https://example.com/file.zip" -o file.zip --sha256 "<64-character SHA-256 digest>"
.\target\release\rustdm.exe queue add "https://example.com/file.zip" -o queued.zip --sha256 "<64-character SHA-256 digest>"
```

Replace the placeholder with the actual digest from a trusted publisher source.
Uppercase and lowercase hexadecimal are accepted. Invalid digests are rejected
before HTTP requests or download state are created. Omit the option to use the
existing size/range/validator checks without publisher checksum verification.
Only SHA-256 is supported in this milestone.

RustDM hashes the assembled bytes in order, including all saved resume data,
before publishing the destination. A mismatch reports expected and actual hashes,
returns a failure, and retains the partial files while leaving the destination
absent. The temporary assembled file is removed. Existing destinations remain
protected. Transfer progress can reach 100% before verification and publication
finish; success is reported only after both complete.

For direct downloads, supply `--sha256` again on every `--resume` invocation;
the direct-download manifest does not store the expected digest. You may correct
a mistyped expected digest when resuming. A correct expected digest cannot repair
corrupt saved bytes: use a fresh output path to download again. Queued jobs persist
the expected digest and automatically reapply it on every attempt. A mismatch
records a failed attempt in history and allows later queued jobs to continue.

## Queue and history

```powershell
.\target\release\rustdm.exe queue add "https://example.com/first.zip" -o "first.zip"
.\target\release\rustdm.exe queue add "https://example.com/second.zip" -o "second.zip"
.\target\release\rustdm.exe queue list
.\target\release\rustdm.exe queue run
.\target\release\rustdm.exe history
```

The queue is stored in `.rustdm/queue.json` in the current directory. Use
`--state-dir "D:\RustDMData"` to select a consistent queue from any directory.
Output paths are resolved when adding a job, so changing directories does not
change its destination. `queue list --json` and `history --json` support scripts.

Ctrl+C pauses the active job and leaves later jobs pending. Run `queue run`
again to resume pending/paused jobs. Failed jobs do not block later jobs; use
`queue retry ID` followed by `queue run` to retry one. History preserves each
terminal attempt, including failed and paused attempts. Only queued downloads
are recorded; direct downloads remain available with either `rustdm URL -o FILE`
or `rustdm download URL -o FILE`.

Queued requests do not persist headers or URL userinfo. Use direct download for
authenticated requests requiring those fields. Source URLs, including query
parameters, are stored in queue/history state. See [queue behavior and recovery](docs/QUEUE.md).

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

All test source files live under [`tests/`](tests/README.md), grouped by component.
Integration tests use a local HTTP server and known binary data, covering
parallel transfers, pause/resume, truncated responses, changed validators,
invalid ranges, fallback behavior, and destination preservation. Queue tests cover
concurrent additions, persistent state, cancellation/recovery, retry history,
exclusive runners, and commands invoked as separate processes. CI is configured
for Windows and Linux. Local validation was performed on Windows; the Actions
results show the status of the configured platforms.

- [Roadmap](ROADMAP.md): CLI -> desktop -> browser -> web.
- [Contributing](CONTRIBUTING.md): checks and Conventional Commit workflow.
- [Changelog](CHANGELOG.md): implemented changes and compatibility notes.
- [Releases](docs/RELEASE.md): portable packages, checksums, and publishing checklist.

This repository contains the new Rust implementation. Build caches, downloaded
files, executable analysis inputs, and extracted vendor code stay outside Git.
