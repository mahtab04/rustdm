# RustDM core

The `rustdm-core` library contains download behavior shared by present and future
applications. Rust code imports it as `rustdm_core`.

Public entry points include `DownloadManager`, `DownloadOptions`,
`ProgressObserver`, `ProgressMeter`, `DownloadQueue`, `QueueRunner`, and
`CancellationToken`. HTTP configuration,
transfer workers, saved state, and final-file publication are encapsulated here.

The crate has no argument parser, terminal renderer, desktop framework, or web
server dependency. See [architecture](../../docs/ARCHITECTURE.md) for its API and
module responsibilities.
The queue service composes the same download manager and exposes job snapshots,
attempt history, and execution events; see [queue behavior](../../docs/QUEUE.md).
