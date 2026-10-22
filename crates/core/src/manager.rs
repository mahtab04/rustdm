use crate::{
    CancellationToken,
    domain::{DownloadOptions, DownloadSummary, Progress, ProgressObserver},
    http::HttpSource,
    state::{DownloadState, part_size},
    transfer::{SegmentWorker, sequential},
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::Url;
use std::{
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};
use tokio::task::JoinSet;

/// Application service shared by the CLI and future desktop/browser adapters.
/// Owns validated-at-execution configuration and composes HTTP, state, and workers.
pub struct DownloadManager {
    options: DownloadOptions,
}
impl DownloadManager {
    pub fn new(options: DownloadOptions) -> Self {
        Self { options }
    }
    pub fn options(&self) -> &DownloadOptions {
        &self.options
    }
    /// A failed/cancelled range transfer retains its parts for a later resume.
    pub async fn download<O: ProgressObserver + 'static>(
        &self,
        cancel: CancellationToken,
        observer: O,
    ) -> Result<DownloadSummary> {
        let options = &self.options;
        ensure!(
            (1..=16).contains(&options.connections),
            "Connections must be between 1 and 16"
        );
        ensure!(options.retries <= 10, "Retries must be between 0 and 10");
        let url = Url::parse(&options.url).context("Invalid download URL")?;
        ensure!(
            matches!(url.scheme(), "http" | "https"),
            "Only HTTP and HTTPS URLs are supported"
        );
        let state = DownloadState::acquire(&options.output, options.resume)?;
        let source = HttpSource::new(options)?;
        let remote = source
            .inspect(&options.url, &cancel, options.retries)
            .await?;
        let manifest = state.load_or_create(options, &remote)?;
        let progress: Arc<dyn ProgressObserver> = Arc::new(observer);
        let mut resumed_bytes = 0;
        let (bytes, part_count) = if manifest.segmented {
            let size = manifest.size.unwrap();
            for (index, segment) in manifest.segments.iter().enumerate() {
                let length = part_size(&state.part_path(index))?;
                ensure!(
                    length <= segment.end - segment.start,
                    "Saved part exceeds its segment"
                );
                resumed_bytes += length;
            }
            progress.on_progress(Progress {
                downloaded: resumed_bytes,
                total: Some(size),
            });
            let downloaded = Arc::new(AtomicU64::new(resumed_bytes));
            let group_cancel = cancel.child_token();
            let mut tasks = JoinSet::new();
            for (index, segment) in manifest.segments.iter().enumerate() {
                let worker = SegmentWorker {
                    client: source.client().clone(),
                    url: remote.final_url.clone(),
                    validator: remote.validator.clone().unwrap(),
                    total: size,
                    path: state.part_path(index),
                    segment: segment.clone(),
                    retries: options.retries,
                    downloaded: downloaded.clone(),
                    cancel: group_cancel.clone(),
                };
                let progress = progress.clone();
                tasks.spawn(async move { worker.run(progress.as_ref()).await });
            }
            let mut error = None;
            while let Some(result) = tasks.join_next().await {
                let result = result.context("Download worker failed").and_then(|r| r);
                if let Err(failure) = result {
                    if error.is_none() {
                        error = Some(failure);
                    }
                    group_cancel.cancel();
                }
            }
            if let Some(error) = error {
                return Err(error.context("Partial files retained; retry with --resume"));
            }
            (size, manifest.segments.len())
        } else {
            let mut result = None;
            for attempt in 0..=options.retries {
                let attempt_result = sequential(
                    source.client(),
                    &remote,
                    &state.part_path(0),
                    &cancel,
                    progress.as_ref(),
                )
                .await;
                let done = attempt_result.is_ok() || cancel.is_cancelled();
                result = Some(attempt_result);
                if done || attempt == options.retries {
                    break;
                }
                tokio::select! {
                    _ = cancel.cancelled() => bail!("Download paused; this server requires a fresh download"),
                    _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))) => {},
                }
            }
            let bytes = result
                .unwrap()
                .context("Single-stream download failed; use a new output path to restart")?;
            (bytes, 1)
        };

        state
            .publish(&options.output, &manifest, part_count, bytes, &cancel)
            .await?;
        progress.on_progress(Progress {
            downloaded: bytes,
            total: Some(bytes),
        });
        state.cleanup(part_count);
        Ok(DownloadSummary {
            bytes,
            connections: part_count,
            resumed_bytes,
            resumable: manifest.segmented,
        })
    }
}
