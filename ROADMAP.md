# Roadmap

Development proceeds in milestones. Each milestone must have working behavior,
tests, documentation, and reviewable Git commits before moving on.

## 1. CLI - v0.1.0 released

- [x] HTTP/HTTPS downloading and range-capability probing.
- [x] Parallel ranges, retries, and progress reporting.
- [x] Cancellation and validated resume from persistent part files.
- [x] Single-stream fallback, empty files, and unknown file sizes.
- [x] Existing-file preservation and exclusive writer locking.
- [x] Shared object-oriented Rust core with focused modules.
- [x] Local HTTP integration tests and project Git conventions.
- [x] Workspace folders for core, CLI, desktop UI, web, and browser adapters.
- [x] Live speed and ETA reporting with reusable transfer metrics.
- [x] Download queue and persistent job history.
- [x] Per-download bandwidth limits shared across connections and retries.
- [x] Optional publisher-provided SHA-256 checksum verification before publication.
- [x] Dynamic dispatch of saved ranges across a bounded connection pool.
- [x] Large-file resume with changed connection limits and weak-ETag/Last-Modified coverage.
- [ ] Split already active slow ranges and expand server interoperability coverage.
- [x] Windows/Linux package automation, archive checksums, and release checklist.
- [x] Published [v0.1.0 CLI](https://github.com/mahtab04/rustdm/releases/tag/v0.1.0) with verified Windows/Linux packages.

Exit criteria: reliable download/resume workflow, queue/history, documented
limitations, and passing supported-platform checks.

## 2. Desktop UI - current milestone

- [x] Native Rust desktop framework: egui/eframe with the shared core.
- [x] Modern dashboard, URL/destination dialog, live progress/speed/ETA, search, and history.
- [x] Persistent queue start/pause/resume/retry through the existing queue runner.
- [x] Destination picker, bandwidth cap, and optional SHA-256 verification.
- [x] Live per-worker connection details with byte counts and transfer states.
- [ ] Remembered preferences, tray behavior, and desktop packaging.

## 3. Browser integration

- [ ] Chrome/Firefox extensions with explicit user-controlled capture.
- [ ] Our own native-messaging host and documented message schema.
- [ ] Transfer URL, referrer, and required request context into the Rust engine.
- [ ] Capture decisions, permissions, and end-to-end browser tests.

## 4. Web interface and advanced features

- [ ] Local web dashboard/API using the same application service.
- [ ] Authentication and access controls for any exposed management endpoint.
- [ ] Scheduling and site/media-specific handling where supported.
- [ ] Revisit additional protocols and cross-platform installers.
