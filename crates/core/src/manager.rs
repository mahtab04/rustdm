use crate::{
    CancellationToken,
    bandwidth::BandwidthLimiter,
    connections::{ConnectionReporter, ConnectionStatus},
    domain::{DownloadOptions, DownloadSummary, Progress, ProgressObserver},
    http::HttpSource,
    state::{DownloadState, part_size},
    transfer::{SegmentWorker, sequential},
};
use anyhow::{Context, Result, ensure};
use reqwest::Url;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
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
        ensure!(
            options.bandwidth_limit != Some(0),
            "Bandwidth limit must be greater than zero"
        );
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
        let limiter = Arc::new(BandwidthLimiter::new(options.bandwidth_limit));
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
            // Parts retain stable indices for assembly/resume; connections claim
            // the next part as soon as their current one completes.
            let parts = Arc::new(
                manifest
                    .segments
                    .iter()
                    .enumerate()
                    .map(|(index, segment)| (state.part_path(index), segment.clone()))
                    .collect::<Vec<_>>(),
            );
            let next = Arc::new(AtomicUsize::new(0));
            for slot in 0..options.connections.min(parts.len()) {
                let parts = parts.clone();
                let next = next.clone();
                let client = source.client().clone();
                let url = remote.final_url.clone();
                let validator = remote.validator.clone().unwrap();
                let retries = options.retries;
                let downloaded = downloaded.clone();
                let cancel = group_cancel.clone();
                let limiter = limiter.clone();
                let progress = progress.clone();
                tasks.spawn(async move {
                    let connection = Arc::new(ConnectionReporter::new(slot + 1));
                    connection.status(progress.as_ref(), ConnectionStatus::Waiting);
                    let result = async {
                        loop {
                            ensure!(!cancel.is_cancelled(), "Download paused");
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some((path, segment)) = parts.get(index) else {
                                return Ok(());
                            };
                            let worker = SegmentWorker {
                                client: client.clone(),
                                url: url.clone(),
                                validator: validator.clone(),
                                total: size,
                                path: path.clone(),
                                segment: segment.clone(),
                                retries,
                                downloaded: downloaded.clone(),
                                cancel: cancel.clone(),
                                limiter: limiter.clone(),
                                connection: connection.clone(),
                            };
                            worker.run(progress.as_ref()).await?;
                        }
                    }
                    .await;
                    connection.finish(&result, &cancel, progress.as_ref());
                    result
                });
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
            let connection = ConnectionReporter::new(1);
            connection.status(progress.as_ref(), ConnectionStatus::Waiting);
            let mut result = None;
            for attempt in 0..=options.retries {
                let attempt_result = sequential(
                    source.client(),
                    &remote,
                    &state.part_path(0),
                    &cancel,
                    progress.as_ref(),
                    &limiter,
                    &connection,
                )
                .await;
                let done = attempt_result.is_ok() || cancel.is_cancelled();
                result = Some(attempt_result);
                if done || attempt == options.retries {
                    break;
                }
                connection.status(progress.as_ref(), ConnectionStatus::Retrying);
                tokio::select! {
                    _ = cancel.cancelled() => { break; },
                    _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))) => {},
                }
            }
            let result = result.unwrap();
            connection.finish(&result, &cancel, progress.as_ref());
            let bytes = result
                .context("Single-stream download failed; use a new output path to restart")?;
            (bytes, 1)
        };

        state
            .publish(
                &options.output,
                &manifest,
                part_count,
                bytes,
                &cancel,
                options.sha256.as_ref(),
            )
            .await?;
        progress.on_progress(Progress {
            downloaded: bytes,
            total: Some(bytes),
        });
        state.cleanup(part_count);
        Ok(DownloadSummary {
            bytes,
            connections: options.connections.min(part_count),
            resumed_bytes,
            resumable: manifest.segmented,
        })
    }
}
