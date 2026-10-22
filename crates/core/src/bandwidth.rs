use crate::CancellationToken;
use anyhow::{Result, bail};
use std::time::Duration;
use tokio::sync::Mutex;

/// One body-consumption budget shared by every connection and retry of a download.
/// Waiting under the lock serializes byte budgets without accumulating idle credit.
pub(crate) struct BandwidthLimiter {
    bytes_per_second: Option<u64>,
    turn: Mutex<()>,
}

impl BandwidthLimiter {
    pub(crate) fn new(bytes_per_second: Option<u64>) -> Self {
        Self {
            bytes_per_second,
            turn: Mutex::new(()),
        }
    }

    pub(crate) fn chunk_size(&self) -> usize {
        self.bytes_per_second
            .map_or(usize::MAX, |rate| (rate / 10).clamp(1, 16_384) as usize)
    }

    pub(crate) async fn consume(&self, bytes: usize, cancel: &CancellationToken) -> Result<()> {
        if cancel.is_cancelled() {
            bail!("Download paused");
        }
        let Some(rate) = self.bytes_per_second else {
            return Ok(());
        };
        // Slices are at most 16 KiB, so this multiplication cannot overflow.
        let nanos = (bytes as u64 * 1_000_000_000).div_ceil(rate);
        tokio::select! {
            biased;
            _ = cancel.cancelled() => bail!("Download paused"),
            _ = async {
                let _turn = self.turn.lock().await;
                tokio::time::sleep(Duration::from_nanos(nanos)).await;
            } => Ok(()),
        }
    }
}
