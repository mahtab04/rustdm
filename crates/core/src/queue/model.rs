use crate::{DownloadOptions, state_directory};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running,
    Completed,
    Failed,
    Paused,
}

impl fmt::Display for JobState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Paused => "paused",
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Attempt {
    pub(crate) started_at: u64,
    pub(crate) finished_at: Option<u64>,
    pub(crate) state: JobState,
    pub(crate) bytes: Option<u64>,
    pub(crate) resumed_bytes: Option<u64>,
    pub(crate) error: Option<String>,
}

/// An immutable external snapshot of one persistent queue job.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueJob {
    pub(crate) id: u64,
    pub(crate) url: String,
    pub(crate) output: PathBuf,
    pub(crate) connections: usize,
    pub(crate) retries: u32,
    pub(crate) resume: bool,
    pub(crate) state: JobState,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) attempts: Vec<Attempt>,
}

impl QueueJob {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn output(&self) -> &Path {
        &self.output
    }
    pub fn state(&self) -> JobState {
        self.state
    }
    pub fn attempt_count(&self) -> usize {
        self.attempts.len()
    }

    pub(crate) fn options(&self) -> DownloadOptions {
        let resume = self.resume || state_directory(&self.output).join("manifest.json").exists();
        DownloadOptions::new(&self.url, &self.output)
            .with_connections(self.connections)
            .with_retries(self.retries)
            .with_resume(resume)
    }
}

/// A completed attempt; timestamps are Unix seconds in UTC.
#[derive(Clone, Debug, Serialize)]
pub struct HistoryEntry {
    pub job_id: u64,
    pub attempt: usize,
    pub url: String,
    pub output: PathBuf,
    pub state: JobState,
    pub started_at: u64,
    pub finished_at: u64,
    pub bytes: Option<u64>,
    pub resumed_bytes: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum QueueEvent {
    Started {
        job_id: u64,
        output: PathBuf,
    },
    Progress {
        job_id: u64,
        progress: crate::Progress,
    },
    Finished {
        job_id: u64,
        state: JobState,
        error: Option<String>,
    },
}

#[derive(Default, Debug)]
pub struct QueueRunSummary {
    pub completed: usize,
    pub failed: usize,
    pub paused: usize,
    pub cancelled: bool,
}
