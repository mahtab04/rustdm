use crate::Progress;
use std::time::{Duration, Instant};

/// A presentation-neutral view of the most recent transfer interval.
#[derive(Clone, Copy, Debug)]
pub struct TransferMetrics {
    pub progress: Progress,
    pub bytes_per_second: Option<f64>,
    pub eta: Option<Duration>,
}

/// Calculates speed from newly received bytes, excluding existing resume data.
/// A counter reset starts a fresh interval, as needed for single-stream retries.
#[derive(Default)]
pub struct ProgressMeter {
    previous: Option<(Instant, u64)>,
}

impl ProgressMeter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, progress: Progress) -> TransferMetrics {
        self.observe_at(progress, Instant::now())
    }

    fn observe_at(&mut self, progress: Progress, now: Instant) -> TransferMetrics {
        let bytes_per_second = self.previous.and_then(|(time, bytes)| {
            let elapsed = now.checked_duration_since(time)?;
            let received = progress.downloaded.checked_sub(bytes)?;
            (elapsed > Duration::ZERO).then(|| received as f64 / elapsed.as_secs_f64())
        });
        // Preserve the baseline when no time elapsed, so a later observation
        // still accounts for all bytes received in that interval.
        if self
            .previous
            .is_none_or(|(time, bytes)| now > time || progress.downloaded < bytes)
        {
            self.previous = Some((now, progress.downloaded));
        }
        let eta = progress.total.and_then(|total| {
            let remaining = total.saturating_sub(progress.downloaded);
            if remaining == 0 {
                return Some(Duration::ZERO);
            }
            let rate = bytes_per_second.filter(|rate| *rate > 0.0)?;
            Duration::try_from_secs_f64(remaining as f64 / rate).ok()
        });
        TransferMetrics {
            progress,
            bytes_per_second,
            eta,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/metrics.rs"]
mod tests;
