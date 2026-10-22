# Desktop UI

Reserved for RustDM's desktop application after the CLI milestone is stable.
The framework has not been chosen and this folder does not yet contain a runnable UI.

The desktop adapter will depend on `crates/core`, use `DownloadManager` for
transfers, observe `Progress`/`TransferMetrics`, and share cancellation and saved
state behavior with the CLI. Queue management belongs in the shared application
layer so the interfaces do not grow separate engines.

See [the roadmap](../../ROADMAP.md) and [architecture](../../docs/ARCHITECTURE.md).
