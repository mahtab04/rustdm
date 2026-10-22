# RustDM core

The `rustdm-core` library contains download behavior shared by present and future
applications. Rust code imports it as `rustdm_core`.

Public entry points include `DownloadManager`, `DownloadOptions`,
`ProgressObserver`, `ProgressMeter`, and `CancellationToken`. HTTP configuration,
transfer workers, saved state, and final-file publication are encapsulated here.

The crate has no argument parser, terminal renderer, desktop framework, or web
server dependency. See [architecture](../../docs/ARCHITECTURE.md) for its API and
module responsibilities.
