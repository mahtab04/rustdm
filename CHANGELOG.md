# Changelog

## Unreleased

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
