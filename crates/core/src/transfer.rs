use crate::{
    CancellationToken,
    bandwidth::BandwidthLimiter,
    domain::{Progress, ProgressObserver, Remote, Segment, Validator},
    http::{identity, range_header, text_header, validator},
    state::part_size,
};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Client, StatusCode, header::*};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
pub(crate) struct SegmentWorker {
    pub(crate) client: Client,
    pub(crate) url: String,
    pub(crate) validator: Validator,
    pub(crate) total: u64,
    pub(crate) path: PathBuf,
    pub(crate) segment: Segment,
    pub(crate) retries: u32,
    pub(crate) downloaded: Arc<AtomicU64>,
    pub(crate) cancel: CancellationToken,
    pub(crate) limiter: Arc<BandwidthLimiter>,
}

impl SegmentWorker {
    pub(crate) async fn run(&self, progress: &dyn ProgressObserver) -> Result<()> {
        for attempt in 0..=self.retries {
            ensure!(!self.cancel.is_cancelled(), "Download paused");
            let current = part_size(&self.path)?;
            ensure!(
                current <= self.segment.end - self.segment.start,
                "Partial file exceeds its segment"
            );
            if current == self.segment.end - self.segment.start {
                return Ok(());
            }
            let result = self.attempt(current, progress).await;
            match result {
                Ok(()) => return Ok(()),
                Err(error) if attempt == self.retries || self.cancel.is_cancelled() => {
                    return Err(error);
                }
                Err(_) => {
                    tokio::select! {
                        _ = self.cancel.cancelled() => bail!("Download paused"),
                        _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))) => {},
                    }
                }
            }
        }
        unreachable!()
    }

    async fn attempt(&self, current: u64, progress: &dyn ProgressObserver) -> Result<()> {
        let start = self.segment.start + current;
        let end = self.segment.end - 1;
        let request = self
            .client
            .get(&self.url)
            .header(RANGE, format!("bytes={start}-{end}"))
            .header(IF_RANGE, &self.validator.value);
        let mut response = tokio::select! {
            _ = self.cancel.cancelled() => bail!("Download paused"),
            response = request.send() => response?,
        };
        ensure!(
            response.status() == StatusCode::PARTIAL_CONTENT,
            "Expected HTTP 206 for a range; received {}. Partial files retained",
            response.status()
        );
        identity(&response)?;
        ensure!(
            range_header(&response)? == (start, end, self.total),
            "Server returned a mismatched Content-Range"
        );
        ensure!(
            text_header(response.headers(), self.validator.header.parse()?).as_deref()
                == Some(self.validator.value.as_str()),
            "Remote validator changed during download"
        );
        let expected = end - start + 1;
        if let Some(length) = response.content_length() {
            ensure!(
                length == expected,
                "Range Content-Length does not match its endpoints"
            );
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let result = async {
            let mut received = 0;
            loop {
                let chunk = tokio::select! {
                    _ = self.cancel.cancelled() => bail!("Download paused"),
                    chunk = response.chunk() => chunk?,
                };
                let Some(chunk) = chunk else {
                    break;
                };
                ensure!(
                    chunk.len() as u64 <= expected - received,
                    "Server sent bytes outside the requested range"
                );
                for slice in chunk.chunks(self.limiter.chunk_size()) {
                    self.limiter.consume(slice.len(), &self.cancel).await?;
                    file.write_all(slice)?;
                    received += slice.len() as u64;
                    let downloaded = self
                        .downloaded
                        .fetch_add(slice.len() as u64, Ordering::Relaxed)
                        + slice.len() as u64;
                    progress.on_progress(Progress {
                        downloaded,
                        total: Some(self.total),
                    });
                }
            }
            ensure!(received == expected, "Range response ended early");
            Ok(())
        }
        .await;
        file.sync_data()?;
        result
    }
}

pub(crate) async fn sequential(
    client: &Client,
    remote: &Remote,
    path: &Path,
    cancel: &CancellationToken,
    progress: &dyn ProgressObserver,
    limiter: &BandwidthLimiter,
) -> Result<u64> {
    let mut response = tokio::select! {
        _ = cancel.cancelled() => bail!("Download paused"),
        response = client.get(&remote.final_url).send() => response?,
    };
    ensure!(
        response.status() == StatusCode::OK,
        "Expected HTTP 200; received {}",
        response.status()
    );
    identity(&response)?;
    if let Some(expected) = &remote.validator {
        ensure!(
            validator(response.headers()).as_ref() == Some(expected),
            "Remote file changed after probe"
        );
    }
    if let (Some(probed), Some(actual)) = (remote.size, response.content_length()) {
        ensure!(probed == actual, "Remote size changed after probe");
    }
    let total = response.content_length().or(remote.size);
    let mut file = File::create(path)?;
    let result = async {
        let mut downloaded = 0u64;
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => bail!("Download paused"),
                chunk = response.chunk() => chunk?,
            };
            let Some(chunk) = chunk else {
                break;
            };
            let next = downloaded
                .checked_add(chunk.len() as u64)
                .context("Download size overflow")?;
            ensure!(
                total.is_none_or(|total| next <= total),
                "Response exceeds the expected file size"
            );
            for slice in chunk.chunks(limiter.chunk_size()) {
                limiter.consume(slice.len(), cancel).await?;
                file.write_all(slice)?;
                downloaded += slice.len() as u64;
                progress.on_progress(Progress { downloaded, total });
            }
        }
        ensure!(
            total.is_none_or(|total| downloaded == total),
            "Response ended before the expected file size"
        );
        Ok(downloaded)
    }
    .await;
    file.sync_data()?;
    result
}
