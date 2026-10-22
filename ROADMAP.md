# Roadmap

Development proceeds in milestones. Each milestone must have working behavior,
tests, documentation, and reviewable Git commits before moving on.

## 1. CLI — current milestone

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
- [ ] Optional publisher-provided checksum verification.
- [ ] Dynamic segment rebalancing and broader interoperability coverage.
- [ ] Versioned CLI release and documented release checklist.

Exit criteria: reliable download/resume workflow, queue/history, documented
limitations, and passing supported-platform checks.

## 2. Desktop UI

- [ ] Choose a desktop framework after the CLI API stabilizes.
- [ ] Add URL dialog and download list with progress, speed, and errors.
- [ ] Pause/resume/cancel and queue management through the existing manager.
- [ ] Settings, destination selection, tray behavior, and packaging.

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
