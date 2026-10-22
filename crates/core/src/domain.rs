use crate::Sha256Checksum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct DownloadOptions {
    pub(crate) url: String,
    pub(crate) output: PathBuf,
    pub(crate) connections: usize,
    pub(crate) retries: u32,
    pub(crate) resume: bool,
    pub(crate) bandwidth_limit: Option<u64>,
    pub(crate) sha256: Option<Sha256Checksum>,
    /// Request context such as Referer, Cookie, or Authorization. Never persisted.
    pub(crate) headers: Vec<(String, String)>,
}

impl DownloadOptions {
    pub fn new(url: impl Into<String>, output: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            output: output.into(),
            connections: 4,
            retries: 3,
            resume: false,
            bandwidth_limit: None,
            sha256: None,
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
pub(crate) struct Validator {
    pub(crate) header: String,
    pub(crate) value: String,
}

#[derive(Debug)]
pub(crate) struct Remote {
    pub(crate) final_url: String,
    pub(crate) size: Option<u64>,
    pub(crate) ranges: bool,
    pub(crate) validator: Option<Validator>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Segment {
    pub(crate) start: u64,
    /// Exclusive endpoint. The HTTP header uses end - 1.
    pub(crate) end: u64,
}

impl DownloadOptions {
    /// Set the maximum number of range connections. Validated when downloading.
    pub fn with_connections(mut self, connections: usize) -> Self {
        self.connections = connections;
        self
    }
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }
    /// Limit aggregate body consumption in bytes per second across all connections.
    /// Zero is rejected when downloading or enqueueing. Omit for unlimited speed.
    pub fn with_bandwidth_limit(mut self, bytes_per_second: u64) -> Self {
        self.bandwidth_limit = Some(bytes_per_second);
        self
    }
    pub fn bandwidth_limit(&self) -> Option<u64> {
        self.bandwidth_limit
    }
    /// Verify the fully assembled file before publishing it to the destination.
    pub fn with_sha256(mut self, checksum: Sha256Checksum) -> Self {
        self.sha256 = Some(checksum);
        self
    }
    pub fn sha256(&self) -> Option<&Sha256Checksum> {
        self.sha256.as_ref()
    }
    pub fn with_resume(mut self, resume: bool) -> Self {
        self.resume = resume;
        self
    }
    /// Add request context. Headers are never written to the download manifest.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn output(&self) -> &std::path::Path {
        &self.output
    }
}

/// UI-independent progress interface. Closures implement it automatically.
pub trait ProgressObserver: Send + Sync {
    fn on_progress(&self, progress: Progress);
}
impl<F: Fn(Progress) + Send + Sync> ProgressObserver for F {
    fn on_progress(&self, progress: Progress) {
        self(progress);
    }
}

impl Segment {
    pub(crate) fn plan(size: u64, connections: usize) -> Vec<Segment> {
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
}
