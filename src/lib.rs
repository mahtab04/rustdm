//! RustIDM's UI-independent download engine.
//! Compose a [`DownloadManager`] with [`DownloadOptions`], an observer, and a
//! cancellation token. HTTP details and persistent state stay encapsulated.

mod domain;
mod http;
mod manager;
mod state;
mod transfer;

pub use domain::{DownloadOptions, DownloadSummary, Progress, ProgressObserver};
pub use manager::DownloadManager;
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
