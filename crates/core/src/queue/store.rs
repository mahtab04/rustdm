use super::model::{Attempt, HistoryEntry, JobState, QueueJob};
use crate::{DownloadOptions, DownloadSummary};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    next_id: u64,
    jobs: Vec<QueueJob>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            jobs: Vec::new(),
        }
    }
}

impl Document {
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported queue format version");
        ensure!(self.next_id > 0, "Invalid next queue ID");
        let mut ids = HashSet::new();
        let mut running = 0;
        for job in &self.jobs {
            ensure!(
                job.id > 0 && job.id < self.next_id && ids.insert(job.id),
                "Invalid queue job identity"
            );
            ensure!(
                job.output.is_absolute(),
                "Queue output path must be absolute"
            );
            validate_url(&job.url)?;
            ensure!(
                (1..=16).contains(&job.connections) && job.retries <= 10,
                "Invalid saved queue options"
            );
            ensure!(
                job.bandwidth_limit != Some(0),
                "Invalid saved bandwidth limit"
            );
            for (index, attempt) in job.attempts.iter().enumerate() {
                ensure!(
                    attempt.state != JobState::Pending,
                    "An attempt cannot be pending"
                );
                ensure!(
                    attempt.finished_at.is_none() == (attempt.state == JobState::Running),
                    "Invalid attempt lifecycle"
                );
                if attempt.state == JobState::Running {
                    ensure!(
                        index + 1 == job.attempts.len() && job.state == JobState::Running,
                        "Invalid running attempt"
                    );
                }
            }
            if job.state == JobState::Running {
                running += 1;
                ensure!(
                    job.attempts
                        .last()
                        .is_some_and(|a| a.state == JobState::Running),
                    "Running job has no active attempt"
                );
            }
        }
        ensure!(running <= 1, "Queue contains multiple running jobs");
        Ok(())
    }

    fn job_mut(&mut self, id: u64) -> Result<&mut QueueJob> {
        self.jobs
            .iter_mut()
            .find(|job| job.id == id)
            .context("Queue job was not found")
    }
}

fn validate_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("Invalid queue URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "Only HTTP and HTTPS URLs can be queued"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Use direct download for URLs with embedded credentials"
    );
    Ok(())
}

fn timestamp() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// Owns persistent job records. Short transactions do not hold a lock during HTTP I/O.
#[derive(Clone)]
pub struct DownloadQueue {
    directory: PathBuf,
}

impl DownloadQueue {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(directory.as_ref()).context("Cannot create queue directory")?;
        Ok(Self {
            directory: fs::canonicalize(directory)?,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn lock(&self, name: &str) -> Result<File> {
        Ok(OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join(name))?)
    }

    pub(crate) fn runner_lock(&self) -> Result<File> {
        let lock = self.lock("runner.lock")?;
        FileExt::try_lock_exclusive(&lock).context("Another queue runner is active")?;
        Ok(lock)
    }

    fn read(&self) -> Result<Document> {
        let path = self.directory.join("queue.json");
        let document: Document = match File::open(path) {
            Ok(file) => serde_json::from_reader(file)
                .context("Cannot read queue.json; existing data was not changed")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Document::default(),
            Err(error) => return Err(error.into()),
        };
        document.validate()?;
        Ok(document)
    }

    fn save(&self, document: &Document) -> Result<()> {
        document.validate()?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer_pretty(&mut temporary, document)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.directory.join("queue.json"))
            .context("Cannot publish queue snapshot")?;
        Ok(())
    }

    fn transact<T>(&self, change: impl FnOnce(&mut Document) -> Result<T>) -> Result<T> {
        let lock = self.lock("queue.lock")?;
        FileExt::lock_exclusive(&lock)?;
        let mut document = self.read()?;
        let result = change(&mut document)?;
        self.save(&document)?;
        Ok(result)
    }

    pub fn enqueue(&self, options: DownloadOptions) -> Result<u64> {
        validate_url(&options.url)?;
        ensure!(
            options.headers.is_empty(),
            "Queued headers are not persisted; use direct download for authenticated requests"
        );
        ensure!(
            (1..=16).contains(&options.connections) && options.retries <= 10,
            "Invalid queue options"
        );
        ensure!(
            options.bandwidth_limit != Some(0),
            "Bandwidth limit must be greater than zero"
        );
        let output = std::path::absolute(&options.output)?;
        ensure!(
            output.file_name().is_some() && !output.exists(),
            "Queued output already exists or has no filename"
        );
        self.transact(|document| {
            ensure!(!document.jobs.iter().any(|job| job.output == output && job.state != JobState::Completed),
                "An unfinished job already uses this output; retry that job or choose another filename");
            let id = document.next_id;
            document.next_id = id.checked_add(1).context("Queue ID limit reached")?;
            let now = timestamp()?;
            document.jobs.push(QueueJob {
                id,
                url: options.url,
                output,
                connections: options.connections,
                retries: options.retries,
                resume: options.resume,
                bandwidth_limit: options.bandwidth_limit,
                state: JobState::Pending,
                created_at: now,
                updated_at: now,
                attempts: Vec::new(),
            });
            Ok(id)
        })
    }

    pub fn jobs(&self) -> Result<Vec<QueueJob>> {
        let lock = self.lock("queue.lock")?;
        FileExt::lock_exclusive(&lock)?;
        Ok(self.read()?.jobs)
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let mut history = Vec::new();
        for job in self.jobs()? {
            for (index, attempt) in job.attempts.iter().enumerate() {
                if let Some(finished_at) = attempt.finished_at {
                    history.push(HistoryEntry {
                        job_id: job.id,
                        attempt: index + 1,
                        url: job.url.clone(),
                        output: job.output.clone(),
                        state: attempt.state,
                        started_at: attempt.started_at,
                        finished_at,
                        bytes: attempt.bytes,
                        resumed_bytes: attempt.resumed_bytes,
                        error: attempt.error.clone(),
                    });
                }
            }
        }
        history.sort_by_key(|entry| (entry.started_at, entry.job_id, entry.attempt));
        Ok(history)
    }

    /// Requeue a failed/paused job without deleting any attempt history or parts.
    pub fn retry(&self, id: u64) -> Result<()> {
        let _runner = self.runner_lock()?;
        self.recover_interrupted()?;
        self.transact(|document| {
            let job = document.job_mut(id)?;
            ensure!(
                matches!(job.state, JobState::Failed | JobState::Paused),
                "Only failed or paused jobs can be retried"
            );
            job.state = JobState::Pending;
            job.updated_at = timestamp()?;
            Ok(())
        })
    }

    pub(crate) fn recover_interrupted(&self) -> Result<()> {
        self.transact(|document| {
            for job in &mut document.jobs {
                if job.state == JobState::Running {
                    let now = timestamp()?;
                    let attempt = job
                        .attempts
                        .last_mut()
                        .context("Missing active queue attempt")?;
                    attempt.state = JobState::Paused;
                    attempt.finished_at = Some(now.max(attempt.started_at));
                    attempt.error =
                        Some("Previous runner stopped before recording completion".into());
                    job.state = JobState::Paused;
                    job.updated_at = now;
                }
            }
            Ok(())
        })
    }

    pub(crate) fn claim_next(&self) -> Result<Option<QueueJob>> {
        self.transact(|document| {
            let Some(job) = document
                .jobs
                .iter_mut()
                .find(|job| matches!(job.state, JobState::Pending | JobState::Paused))
            else {
                return Ok(None);
            };
            let now = timestamp()?;
            job.state = JobState::Running;
            job.updated_at = now;
            job.attempts.push(Attempt {
                started_at: now,
                finished_at: None,
                state: JobState::Running,
                bytes: None,
                resumed_bytes: None,
                error: None,
            });
            Ok(Some(job.clone()))
        })
    }

    pub(crate) fn finish(
        &self,
        id: u64,
        state: JobState,
        summary: Option<&DownloadSummary>,
        error: Option<String>,
    ) -> Result<()> {
        self.transact(|document| {
            ensure!(
                matches!(
                    state,
                    JobState::Completed | JobState::Failed | JobState::Paused
                ),
                "Invalid terminal job state"
            );
            let job = document.job_mut(id)?;
            ensure!(job.state == JobState::Running, "Job is no longer running");
            let attempt = job
                .attempts
                .last_mut()
                .context("Missing active queue attempt")?;
            let now = timestamp()?;
            attempt.state = state;
            attempt.finished_at = Some(now.max(attempt.started_at));
            attempt.bytes = summary.map(|summary| summary.bytes);
            attempt.resumed_bytes = summary.map(|summary| summary.resumed_bytes);
            attempt.error = error;
            job.state = state;
            job.updated_at = now;
            Ok(())
        })
    }
}
