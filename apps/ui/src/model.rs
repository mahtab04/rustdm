use anyhow::{Context, Result, ensure};
use rustdm_core::{DownloadOptions, JobState};
use std::path::PathBuf;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Filter {
    #[default]
    All,
    Active,
    Completed,
    History,
}

impl Filter {
    pub(crate) fn accepts(self, state: JobState) -> bool {
        match self {
            Self::All => true,
            Self::Active => matches!(
                state,
                JobState::Pending | JobState::Running | JobState::Paused
            ),
            Self::Completed => state == JobState::Completed,
            Self::History => false,
        }
    }
}

pub(crate) struct DownloadForm {
    pub(crate) url: String,
    pub(crate) output: String,
    pub(crate) connections: usize,
    pub(crate) rate: String,
    pub(crate) sha256: String,
    pub(crate) resume: bool,
    pub(crate) error: Option<String>,
}

impl Default for DownloadForm {
    fn default() -> Self {
        Self {
            url: String::new(),
            output: String::new(),
            connections: 4,
            rate: String::new(),
            sha256: String::new(),
            resume: false,
            error: None,
        }
    }
}

impl DownloadForm {
    pub(crate) fn options(&self) -> Result<DownloadOptions> {
        ensure!(
            !self.url.trim().is_empty(),
            "Enter a direct HTTP or HTTPS file URL"
        );
        ensure!(
            !self.output.trim().is_empty(),
            "Choose a destination filename"
        );
        let output = PathBuf::from(self.output.trim());
        ensure!(
            !output.is_dir(),
            "Choose a filename inside the destination folder"
        );
        let mut options = DownloadOptions::new(self.url.trim(), output)
            .with_connections(self.connections)
            .with_resume(self.resume);
        if !self.rate.trim().is_empty() {
            let rate: u64 = self
                .rate
                .trim()
                .parse()
                .context("Speed limit must be a positive number in KiB/s")?;
            ensure!(rate > 0, "Speed limit must be greater than zero");
            options = options
                .with_bandwidth_limit(rate.checked_mul(1024).context("Speed limit is too large")?);
        }
        if !self.sha256.trim().is_empty() {
            options = options.with_sha256(self.sha256.trim().parse()?);
        }
        Ok(options)
    }
}

pub(crate) fn bytes(value: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut number = value as f64;
    let mut index = 0;
    while number >= 1024.0 && index < units.len() - 1 {
        number /= 1024.0;
        index += 1;
    }
    format!("{number:.1} {}", units[index])
}

#[cfg(test)]
#[path = "../../../tests/ui/model.rs"]
mod tests;
