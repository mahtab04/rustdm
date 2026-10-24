//! RustDM's UI-independent download engine.
//! Compose a [`DownloadManager`] with [`DownloadOptions`], an observer, and a
//! cancellation token. HTTP details and persistent state stay encapsulated.

mod bandwidth;
mod checksum;
mod connections;
mod domain;
mod http;
mod manager;
mod metrics;
mod queue;
mod state;
mod transfer;

pub use checksum::Sha256Checksum;
pub use connections::{ConnectionProgress, ConnectionStatus};
pub use domain::{DownloadOptions, DownloadSummary, Progress, ProgressObserver};
pub use manager::DownloadManager;
pub use metrics::{ProgressMeter, TransferMetrics};
pub use queue::{
    DownloadQueue, HistoryEntry, JobState, QueueEvent, QueueJob, QueueRunSummary, QueueRunner,
};
pub use state::state_directory;
pub use tokio_util::sync::CancellationToken;

/// Convenience entry point; the same service is available as [`DownloadManager`].
pub async fn download<O: Fn(Progress) + Send + Sync + 'static>(
    options: &DownloadOptions,
    cancel: CancellationToken,
    observer: O,
) -> anyhow::Result<DownloadSummary> {
    DownloadManager::new(options.clone())
        .download(cancel, observer)
        .await
}
