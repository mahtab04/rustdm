use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use reqwest::{Client, Response, StatusCode, Url, header::*};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::task::JoinSet;
pub use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    pub url: String,
    pub output: PathBuf,
    pub connections: usize,
    pub retries: u32,
    pub resume: bool,
    /// Request context such as Referer, Cookie, or Authorization. Never persisted.
    pub headers: Vec<(String, String)>,
}

impl DownloadOptions {
    pub fn new(url: impl Into<String>, output: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            output: output.into(),
            connections: 4,
            retries: 3,
            resume: false,
            headers: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug)]
pub struct DownloadSummary {
    pub bytes: u64,
    pub connections: usize,
    pub resumed_bytes: u64,
    pub resumable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Validator {
    header: String,
    value: String,
}

#[derive(Debug)]
struct Remote {
    final_url: String,
    size: Option<u64>,
    ranges: bool,
    validator: Option<Validator>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Segment {
    start: u64,
    /// Exclusive endpoint. The HTTP header uses end - 1.
    end: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    url: String,
    final_url: String,
    size: Option<u64>,
    validator: Option<Validator>,
    segmented: bool,
    segments: Vec<Segment>,
}

/// Location of the recoverable partial files. No authentication headers are stored.
pub fn state_directory(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".rustdm");
    PathBuf::from(name)
}

fn text_header(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

fn validator(headers: &HeaderMap) -> Option<Validator> {
    if let Some(value) = text_header(headers, ETAG)
        .filter(|v| v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
    {
        return Some(Validator {
            header: "etag".into(),
            value,
        });
    }
    text_header(headers, LAST_MODIFIED).map(|value| Validator {
        header: "last-modified".into(),
        value,
    })
}

fn identity(response: &Response) -> Result<()> {
    if let Some(value) = response.headers().get(CONTENT_ENCODING) {
        ensure!(
            value == "identity",
            "Server returned an encoded representation despite Accept-Encoding: identity"
        );
    }
    Ok(())
}

fn parse_content_range(value: &str) -> Result<(u64, u64, u64)> {
    let rest = value
        .strip_prefix("bytes ")
        .context("Invalid Content-Range unit")?;
    let (bounds, total) = rest
        .split_once('/')
        .context("Missing Content-Range total")?;
    let (start, end) = bounds
        .split_once('-')
        .context("Missing Content-Range endpoints")?;
    let (start, end, total): (u64, u64, u64) = (start.parse()?, end.parse()?, total.parse()?);
    ensure!(start <= end && end < total, "Invalid Content-Range bounds");
    Ok((start, end, total))
}

fn range_header(response: &Response) -> Result<(u64, u64, u64)> {
    parse_content_range(
        response
            .headers()
            .get(CONTENT_RANGE)
            .context("Partial response omitted Content-Range")?
            .to_str()?,
    )
}

async fn probe(client: &Client, url: &str, cancel: &CancellationToken) -> Result<Remote> {
    let response = tokio::select! {
        _ = cancel.cancelled() => bail!("Download paused"),
        response = client.get(url).header(RANGE, "bytes=0-0").send() => response?,
    };
    identity(&response)?;
    let final_url = response.url().to_string();
    let validator = validator(response.headers());
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE
        && response
            .headers()
            .get(CONTENT_RANGE)
            .is_some_and(|v| v == "bytes */0")
    {
        return Ok(Remote {
            final_url,
            size: Some(0),
            ranges: false,
            validator,
        });
    }
    if response.status() == StatusCode::PARTIAL_CONTENT {
        let (start, end, total) = range_header(&response)?;
        ensure!(
            start == 0 && end == 0,
            "Server ignored the probe's requested range"
        );
        return Ok(Remote {
            final_url,
            size: Some(total),
            ranges: true,
            validator,
        });
    }
    ensure!(
        response.status() == StatusCode::OK,
        "Probe returned HTTP {}",
        response.status()
    );
    Ok(Remote {
        final_url,
        size: response.content_length(),
        ranges: false,
        validator,
    })
}

fn segments(size: u64, connections: usize) -> Vec<Segment> {
    if size == 0 {
        return Vec::new();
    }
    let count = u64::try_from(connections).unwrap_or(16).min(size);
    let base = size / count;
    let extra = size % count;
    let mut start = 0;
    (0..count)
        .map(|i| {
            let end = start + base + u64::from(i < extra);
            let segment = Segment { start, end };
            start = end;
            segment
        })
        .collect()
}

fn part_path(state: &Path, index: usize) -> PathBuf {
    state.join(format!("part-{index:03}"))
}

fn part_size(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => {
            ensure!(metadata.is_file(), "Partial path is not a regular file");
            Ok(metadata.len())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn validate_saved(saved: &Manifest, options: &DownloadOptions, remote: &Remote) -> Result<()> {
    ensure!(saved.version == 1, "Unsupported partial-download format");
    ensure!(
        saved.url == options.url && saved.final_url == remote.final_url,
        "Resume URL differs from the saved download"
    );
    ensure!(
        saved.segmented && remote.ranges && saved.validator.is_some(),
        "This download cannot safely resume: byte ranges and a server validator are required. Use a new output path"
    );
    ensure!(
        saved.size == remote.size && saved.validator == remote.validator,
        "Remote file changed; refusing to combine it with saved data. Use a new output path"
    );
    let size = saved.size.context("Missing saved total")?;
    ensure!(
        !saved.segments.is_empty() && saved.segments.len() <= 16,
        "Invalid saved segment count"
    );
    let mut next = 0;
    for segment in &saved.segments {
        ensure!(
            segment.start == next && segment.end > segment.start && segment.end <= size,
            "Invalid saved segment layout"
        );
        next = segment.end;
    }
    ensure!(next == size, "Saved segments do not cover the entire file");
    Ok(())
}

struct Worker {
    client: Client,
    url: String,
    validator: Validator,
    total: u64,
    path: PathBuf,
    segment: Segment,
    retries: u32,
    downloaded: Arc<AtomicU64>,
    cancel: CancellationToken,
}

impl Worker {
    async fn run(&self, progress: &(dyn Fn(Progress) + Send + Sync)) -> Result<()> {
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

    async fn attempt(
        &self,
        current: u64,
        progress: &(dyn Fn(Progress) + Send + Sync),
    ) -> Result<()> {
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
                file.write_all(&chunk)?;
                received += chunk.len() as u64;
                let downloaded = self
                    .downloaded
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed)
                    + chunk.len() as u64;
                progress(Progress {
                    downloaded,
                    total: Some(self.total),
                });
            }
            ensure!(received == expected, "Range response ended early");
            Ok(())
        }
        .await;
        file.sync_data()?;
        result
    }
}

async fn sequential(
    client: &Client,
    remote: &Remote,
    path: &Path,
    cancel: &CancellationToken,
    progress: &(dyn Fn(Progress) + Send + Sync),
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
            downloaded = downloaded
                .checked_add(chunk.len() as u64)
                .context("Download size overflow")?;
            ensure!(
                total.is_none_or(|total| downloaded <= total),
                "Response exceeds the expected file size"
            );
            file.write_all(&chunk)?;
            progress(Progress { downloaded, total });
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

/// Download without overwriting an existing destination. A cancelled or failed
/// range download keeps its manifest and part files for a later --resume call.
pub async fn download<F>(
    options: &DownloadOptions,
    cancel: CancellationToken,
    progress: F,
) -> Result<DownloadSummary>
where
    F: Fn(Progress) + Send + Sync + 'static,
{
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
    ensure!(
        !options.output.exists(),
        "Output already exists; choose a different filename"
    );
    let parent = options
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).context("Cannot create output directory")?;
    let state = state_directory(&options.output);
    fs::create_dir_all(&state)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.join("lock"))?;
    FileExt::try_lock_exclusive(&lock).context("Another download is using this output")?;
    ensure!(
        !options.output.exists(),
        "Output was created by another download"
    );
    let manifest_path = state.join("manifest.json");
    ensure!(
        !manifest_path.exists() || options.resume,
        "Partial download exists. Run again with --resume"
    );
    let mut headers = HeaderMap::new();
    for (name, value) in &options.headers {
        let name: HeaderName = name.parse().context("Invalid request header name")?;
        ensure!(
            name != RANGE && name != IF_RANGE && name != ACCEPT_ENCODING && name != CONTENT_LENGTH,
            "Range, If-Range, Accept-Encoding and Content-Length are managed by the engine"
        );
        let mut value: HeaderValue = value.parse().context("Invalid request header value")?;
        value.set_sensitive(true);
        headers.append(name, value);
    }
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    if !headers.contains_key(USER_AGENT) {
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("rustdm/", env!("CARGO_PKG_VERSION"))),
        );
    }
    let client = Client::builder()
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let mut inspection = probe(&client, &options.url, &cancel).await;
    for attempt in 0..options.retries {
        if inspection.is_ok() || cancel.is_cancelled() {
            break;
        }
        tokio::select! {
            _ = cancel.cancelled() => bail!("Download paused"),
            _ = tokio::time::sleep(Duration::from_millis(250 * (1 << attempt.min(4)))) => {},
        }
        inspection = probe(&client, &options.url, &cancel).await;
    }
    let remote = inspection.context("Cannot inspect download")?;
    let segmented =
        remote.ranges && remote.validator.is_some() && remote.size.is_some_and(|n| n > 0);
    let manifest = if manifest_path.exists() {
        let saved: Manifest = serde_json::from_reader(File::open(&manifest_path)?)
            .context("Cannot read saved download manifest")?;
        validate_saved(&saved, options, &remote)?;
        saved
    } else {
        ensure!(
            fs::read_dir(&state)?.all(|entry| entry.is_ok_and(|entry| entry.file_name() == "lock")),
            "Unrecognized files exist in the download sidecar; use a new output path"
        );
        let manifest = Manifest {
            version: 1,
            url: options.url.clone(),
            final_url: remote.final_url.clone(),
            size: remote.size,
            validator: remote.validator.clone(),
            segmented,
            segments: if segmented {
                segments(remote.size.unwrap(), options.connections)
            } else {
                Vec::new()
            },
        };
        let mut temp = tempfile::NamedTempFile::new_in(&state)?;
        serde_json::to_writer_pretty(&mut temp, &manifest)?;
        temp.as_file().sync_all()?;
        temp.persist_noclobber(&manifest_path)
            .context("Cannot save download manifest")?;
        manifest
    };
    let progress: Arc<dyn Fn(Progress) + Send + Sync> = Arc::new(progress);
    let mut resumed_bytes = 0;
    let (bytes, part_count) = if manifest.segmented {
        let size = manifest.size.unwrap();
        for (index, segment) in manifest.segments.iter().enumerate() {
            let length = part_size(&part_path(&state, index))?;
            ensure!(
                length <= segment.end - segment.start,
                "Saved part exceeds its segment"
            );
            resumed_bytes += length;
        }
        progress(Progress {
            downloaded: resumed_bytes,
            total: Some(size),
        });
        let downloaded = Arc::new(AtomicU64::new(resumed_bytes));
        let group_cancel = cancel.child_token();
        let mut tasks = JoinSet::new();
        for (index, segment) in manifest.segments.iter().enumerate() {
            let worker = Worker {
                client: client.clone(),
                url: remote.final_url.clone(),
                validator: remote.validator.clone().unwrap(),
                total: size,
                path: part_path(&state, index),
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
                &client,
                &remote,
                &part_path(&state, 0),
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
    ensure!(
        !cancel.is_cancelled(),
        "Download paused before assembly; partial files retained"
    );
    let mut assembled = tempfile::NamedTempFile::new_in(&state)?;
    let mut copied = 0;
    let mut buffer = [0u8; 64 * 1024];
    for index in 0..part_count {
        let path = part_path(&state, index);
        let mut part = File::open(&path)?;
        if manifest.segmented {
            ensure!(
                part.metadata()?.len()
                    == manifest.segments[index].end - manifest.segments[index].start,
                "Part size changed before assembly"
            );
        }
        loop {
            ensure!(
                !cancel.is_cancelled(),
                "Download paused during assembly; partial files retained"
            );
            let count = part.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            assembled.write_all(&buffer[..count])?;
            copied += count as u64;
            tokio::task::yield_now().await;
        }
    }
    ensure!(
        copied == bytes,
        "Assembled size does not match downloaded size"
    );
    assembled.as_file().sync_all()?;
    assembled
        .persist_noclobber(&options.output)
        .context("Cannot publish output without overwriting an existing file")?;
    progress(Progress {
        downloaded: bytes,
        total: Some(bytes),
    });
    // Only remove known files we created. Unrelated sidecar contents are retained.
    for index in 0..part_count {
        let _ = fs::remove_file(part_path(&state, index));
    }
    let _ = fs::remove_file(&manifest_path);
    drop(lock);
    let _ = fs::remove_file(state.join("lock"));
    let _ = fs::remove_dir(&state);
    Ok(DownloadSummary {
        bytes,
        connections: part_count,
        resumed_bytes,
        resumable: manifest.segmented,
    })
}
