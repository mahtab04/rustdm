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

## 2. Desktop UI - preview

- [x] Native Rust desktop framework: egui/eframe with the shared core.
- [x] Modern dashboard, URL/destination dialog, live progress/speed/ETA, search, and history.
- [x] Persistent queue start/pause/resume/retry through the existing queue runner.
- [x] Destination picker, bandwidth cap, and optional SHA-256 verification.
- [x] Live per-worker connection details with byte counts and transfer states.
- [x] Remembered transfer defaults and initial file picker folder.
- [x] Safe shutdown during pending additions/pickers and visible worker/startup errors.
- [x] Portable desktop preview packages with checksums and extracted-binary CI checks.
- [x] Windows tray show/start/pause/quit, explicit hide, and optional close-to-tray.
- [ ] Linux/macOS tray adapters, remembered window position/size, and installer.

## 3. Browser integration - current milestone

- [x] Chromium/Firefox development extensions with selected-link review and pasted URLs.
- [x] Rust native messaging host with bounded framing, caller checks, and documented schema.
- [x] Enqueue public direct-file URLs into the shared desktop/CLI queue.
- [x] Current-user Windows/Linux setup and portable browser preview packages.
- [x] Protocol, extension/host/CLI HTTP tests, and real Edge review/native-message smoke check.
- [ ] Referrer/cookie/header handoff with defined persistence and permission rules.
- [ ] Automatic interception, signed store releases, and native Firefox UI validation.

## 4. Web interface and advanced features

- [ ] Local web dashboard/API using the same application service.
- [ ] Authentication and access controls for any exposed management endpoint.
- [ ] Scheduling and site/media-specific handling where supported.
- [ ] Revisit additional protocols and cross-platform installers.
