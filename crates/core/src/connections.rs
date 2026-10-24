use crate::{CancellationToken, ProgressObserver};
use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};

/// State of one logical transfer worker, rather than an underlying pooled TCP socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Waiting,
    Sending,
    Receiving,
    Retrying,
    Completed,
    Paused,
    Failed,
}

/// Newly written response-body bytes for a worker during this execution.
/// Counts exclude saved resume bytes and probing, and accumulate across assigned ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionProgress {
    pub id: usize,
    pub downloaded: u64,
    pub status: ConnectionStatus,
}

pub(crate) struct ConnectionReporter {
    id: usize,
    downloaded: AtomicU64,
}

impl ConnectionReporter {
    pub(crate) fn new(id: usize) -> Self {
        Self {
            id,
            downloaded: AtomicU64::new(0),
        }
    }
    pub(crate) fn status(&self, observer: &dyn ProgressObserver, status: ConnectionStatus) {
        observer.on_connection(ConnectionProgress {
            id: self.id,
            downloaded: self.downloaded.load(Ordering::Relaxed),
            status,
        });
    }
    pub(crate) fn received(&self, bytes: usize, observer: &dyn ProgressObserver) {
        self.downloaded.fetch_add(bytes as u64, Ordering::Relaxed);
        self.status(observer, ConnectionStatus::Receiving);
    }
    pub(crate) fn finish<T>(
        &self,
        result: &Result<T>,
        cancel: &CancellationToken,
        observer: &dyn ProgressObserver,
    ) {
        let status = if result.is_ok() {
            ConnectionStatus::Completed
        } else if cancel.is_cancelled() {
            ConnectionStatus::Paused
        } else {
            ConnectionStatus::Failed
        };
        self.status(observer, status);
    }
}
