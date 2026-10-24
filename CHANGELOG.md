# Changelog

## Unreleased

### Live connection details

- Added a collapsible connection table on desktop download cards, showing each
  worker's newly downloaded bytes and request/receive/retry/terminal state.
- Added optional `ProgressObserver::on_connection` telemetry and queue connection
  events. Worker IDs remain stable while processing multiple dynamic ranges.
- Coalesced live snapshots separately from lifecycle events and retained final
  snapshots when the queue moves to another job. Saved resume bytes are excluded
  from per-worker counters; single-stream fallback reports one worker.
- Added HTTP coverage for dynamic ranges, retry, pause/resume byte accounting,
  failures, and final desktop telemetry delivery.

### Desktop preview

- Added the native `rustdm-ui` workspace application with a modern dark dashboard,
  download cards, filters, search, and attempt history.
- Added a URL/destination dialog and native file picker, plus connection, KiB/s
  bandwidth, SHA-256, and existing-part resume options.
- Connected start/pause/resume/retry to the persistent core queue on a background
  Tokio runtime, coalesced progress, and graceful pause before window closure.
- Added a persistent per-user queue directory and `--state-dir` for sharing a
  queue with the CLI. Published v0.1.0 CLI assets remain unchanged.
- Added desktop form/backend tests under repository-root `tests/ui` and Linux
  desktop build dependencies in CI. Desktop release packaging is future work.

## [0.1.0](https://github.com/mahtab04/rustdm/releases/tag/v0.1.0) - 2026-10-22

- Published the first CLI release with verified Windows x86-64 ZIP and Linux
  x86-64 tar.gz downloads, SHA-256 files, and source/build metadata.

### CLI release preparation

- Added native Windows ZIP and Linux tar.gz packages with archive SHA-256 files
  and build metadata identifying the source, target, compiler, and binary hash.
- CI now verifies and smoke-tests extracted packages and retains branch artifacts.
- Added reusable CI and a version-tag workflow that creates a draft release only
  after both platforms pass. Tags must match the workspace/executable version.
- Added v0.1.0 release notes and a checklist for validating and publishing drafts.

### Dynamic range scheduling

- Large downloads now plan multiple saved ranges per connection and dispatch
  pending work to free workers, while respecting `--connections` on resume too.
- Kept stable part indices, retries, shared bandwidth limits, and final checksum
  verification. Planning is bounded at 128 parts with a 256 KiB size threshold.
- Retained support for older manifests; older builds reject new manifests with
  more than 16 parts. Active range boundaries are not split during transfer.
- Added blocked-range scheduling, large-file resume, byte-coverage planning, and
  weak-ETag/Last-Modified interoperability tests.
- Fixed the threaded HTTP fixture to use blocking accepted sockets on Windows,
  preventing inherited nonblocking mode from interrupting larger test responses.

### Checksum verification

- Added optional `--sha256 HEX` verification for direct and queued downloads,
  with strict 64-character hexadecimal validation and case normalization.
- Added the shared `Sha256Checksum` value type and `DownloadOptions::with_sha256`.
- Hash the complete assembly, including saved resume bytes, before publication;
  mismatches retain parts, discard temporary assembly, and report failure.
- Persist optional queue digests, keeping older records readable. Queue mismatches
  are failed attempts and do not block later jobs.
- Added known hash vectors, local HTTP verification, corrupt-resume safety,
  CLI exit behavior, queue history, and malformed saved-digest coverage.

### Bandwidth limits

- Added `--limit-rate BYTES_PER_SECOND` to direct downloads and `queue add`, with
  positive-integer validation and unlimited speed when omitted.
- Added an encapsulated per-download limiter shared by all range connections and
  retries, also used by fallback and unknown-length streams. Cancellation
  interrupts limiter waits; resume charges only newly transferred data.
- Persisted optional queue limits while keeping older queue records readable.
- Added local HTTP pacing, cancellation/resume, CLI, and persistence coverage.

### Test organization

- Centralized all test source files under repository-root `tests/`, grouped by
  core/queue and CLI, with reserved UI, web, and browser test folders.
- Preserved private unit-test access and Cargo integration-test discovery, with
  contributor guidance for adding suites.

### Queue and history

- Added shared `DownloadQueue` and `QueueRunner` services with versioned,
  atomic snapshots, stable IDs, absolute destinations, and per-attempt history.
- Added `queue add`, `queue list`, `queue run`, `queue retry`, and `history`,
  with JSON output and a global `--state-dir` option.
- Added sequential job execution through the existing manager, recovery of
  interrupted sessions, pause/resume, and continued processing after failed jobs.
- Kept direct-download syntax and added an explicit `download` command.
- Kept credentials out of queue state by rejecting queued headers and URL userinfo.
- Added persistence, locking, retry, recovery, and CLI subprocess coverage.
- Moved the assembly buffer to the heap to prevent Windows stack overflow when
  download futures are composed through the queue and CLI execution layers.

### Workspace and RustDM rename

- Renamed the project, executable, and GitHub repository to RustDM (Rust Download
  Manager). Existing Git history is preserved.
- Split the Cargo workspace into `rustdm-core` under `crates/core` and the
  `rustdm` application under `apps/cli`, with shared versions and dependencies.
- Added documented folders for future desktop UI, web, and browser integration.
- Added reusable speed/ETA measurements and a terminal progress renderer.
- Coalesced terminal progress events through a watch channel to retain completion
  without an unbounded queue of updates.
- New downloads use `.rustdm` state directories. Existing `.rustidm` directories
  can resume in place when a current-name directory does not already exist.

### Added

- RustDM CLI for HTTP/HTTPS downloads with parallel byte ranges, retries,
  cancellation, progress, and resumable partial files.
- Range and validator checks, single-stream fallback, exclusive output locking,
  and publication that preserves an existing destination.
- `DownloadManager`, builder-style `DownloadOptions`, and a `ProgressObserver`
  trait for shared use by future interfaces.
- HTTP integration tests, project conventions, milestone roadmap, and CI for
  Windows and Linux.

### Changed

- Split the engine into domain, HTTP, state, transfer, and manager modules.
