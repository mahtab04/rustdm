use rustdm_core::{Progress, ProgressMeter};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

/// Terminal adapter; transfer measurement lives in the shared core.
pub(crate) struct ProgressRenderer {
    meter: ProgressMeter,
    last_render: Instant,
    initialized: bool,
}

impl ProgressRenderer {
    pub(crate) fn new() -> Self {
        Self {
            meter: ProgressMeter::new(),
            last_render: Instant::now(),
            initialized: false,
        }
    }

    pub(crate) fn render(&mut self, progress: Progress) {
        let complete = progress.total == Some(progress.downloaded);
        let display = self.last_render.elapsed() >= Duration::from_millis(200) || complete;
        if !display && self.initialized {
            return;
        }
        let metrics = self.meter.observe(progress);
        self.initialized = true;
        if !display {
            return;
        }
        self.last_render = Instant::now();
        let speed = metrics
            .bytes_per_second
            .map(|rate| format!("{} /s", format_size(rate as u64)))
            .unwrap_or_else(|| "-- /s".into());
        let eta = metrics
            .eta
            .map(|remaining| {
                let seconds = remaining.as_secs();
                format!(
                    "{:02}:{:02}:{:02}",
                    seconds / 3600,
                    seconds / 60 % 60,
                    seconds % 60
                )
            })
            .unwrap_or_else(|| "--:--:--".into());
        match progress.total {
            Some(total) => eprint!(
                "\r{} / {} ({:.1}%) | {speed} | ETA {eta}          ",
                format_size(progress.downloaded),
                format_size(total),
                if total == 0 {
                    100.0
                } else {
                    progress.downloaded as f64 * 100.0 / total as f64
                }
            ),
            None => eprint!(
                "\r{} downloaded | {speed} | ETA {eta}          ",
                format_size(progress.downloaded)
            ),
        }
        let _ = io::stderr().flush();
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}
