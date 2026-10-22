# Changelog

## Unreleased

### Added

- RustIDM CLI for HTTP/HTTPS downloads with parallel byte ranges, retries,
  cancellation, progress, and resumable partial files.
- Range and validator checks, single-stream fallback, exclusive output locking,
  and publication that preserves an existing destination.
- `DownloadManager`, builder-style `DownloadOptions`, and a `ProgressObserver`
  trait for shared use by future interfaces.
- HTTP integration tests, project conventions, milestone roadmap, and CI for
  Windows and Linux.

### Changed

- Split the engine into domain, HTTP, state, transfer, and manager modules.
- Renamed the package and executable from `rustdm` to `rustidm`. New downloads
  use `.rustidm` sidecar directories; old `.rustdm` sidecars are not migrated.
