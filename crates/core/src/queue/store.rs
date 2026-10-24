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
                ensure!(!job.removed, "A removed job cannot be running");
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
            ensure!(!document.jobs.iter().any(|job| !job.removed && job.output == output && job.state != JobState::Completed),
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
                sha256: options.sha256,
                state: JobState::Pending,
                held: false,
                removed: false,
                created_at: now,
                updated_at: now,
                attempts: Vec::new(),
            });
            Ok(id)
        })
    }

    pub fn jobs(&self) -> Result<Vec<QueueJob>> {
        Ok(self
            .all_jobs()?
            .into_iter()
            .filter(|job| !job.removed)
            .collect())
    }

    fn all_jobs(&self) -> Result<Vec<QueueJob>> {
        let lock = self.lock("queue.lock")?;
        FileExt::lock_exclusive(&lock)?;
        Ok(self.read()?.jobs)
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let mut history = Vec::new();
        for job in self.all_jobs()? {
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
            ensure!(!job.removed, "Job was removed");
            ensure!(
                matches!(job.state, JobState::Failed | JobState::Paused),
                "Only failed or paused jobs can be retried"
            );
            job.state = JobState::Pending;
            job.held = false;
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
        self.claim(None)
    }

    pub(crate) fn claim(&self, selected: Option<u64>) -> Result<Option<QueueJob>> {
        self.transact(|document| {
            if let Some(id) = selected {
                let job = document.job_mut(id)?;
                ensure!(
                    !job.removed
                        && matches!(
                            job.state,
                            JobState::Pending | JobState::Paused | JobState::Failed
                        ),
                    "Selected job cannot be started"
                );
            }
            let Some(job) = document.jobs.iter_mut().find(|job| {
                !job.removed
                    && match selected {
                        Some(id) => job.id == id,
                        None => {
                            !job.held && matches!(job.state, JobState::Pending | JobState::Paused)
                        }
                    }
            }) else {
                return Ok(None);
            };
            let now = timestamp()?;
            job.state = JobState::Running;
            job.held = false;
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
            if state == JobState::Completed {
                job.held = false;
            }
            job.updated_at = now;
            Ok(())
        })
    }

    /// Hold a waiting job without stopping any other transfer.
    pub fn pause(&self, id: u64) -> Result<()> {
        self.hold(id, false)
    }

    pub(crate) fn hold(&self, id: u64, active: bool) -> Result<()> {
        self.transact(|document| {
            let job = document.job_mut(id)?;
            ensure!(
                !job.removed
                    && (matches!(job.state, JobState::Pending | JobState::Paused)
                        || (active && job.state == JobState::Running)),
                "Job cannot be paused by this controller"
            );
            job.held = true;
            if job.state == JobState::Pending {
                job.state = JobState::Paused;
            }
            job.updated_at = timestamp()?;
            Ok(())
        })
    }

    /// Remove a stopped job from the visible queue, retaining files and attempt history.
    pub fn remove(&self, id: u64) -> Result<()> {
        self.transact(|document| {
            let job = document.job_mut(id)?;
            ensure!(
                job.state != JobState::Running,
                "Pause the download before removing it"
            );
            job.removed = true;
            job.updated_at = timestamp()?;
            Ok(())
        })
    }

    /// Delete a stopped job's completed file or verified partial data, then remove its row.
    pub fn delete_files(&self, id: u64) -> Result<()> {
        self.transact(|document| {
            let job = document
                .jobs
                .iter()
                .find(|job| job.id == id && !job.removed)
                .context("Job was not found")?;
            ensure!(
                job.state != JobState::Running,
                "Pause the download before deleting files"
            );
            ensure!(
                !document
                    .jobs
                    .iter()
                    .any(|other| other.id != id && !other.removed && other.output == job.output),
                "Another queue job uses this destination"
            );
            if job.state == JobState::Completed {
                match fs::symlink_metadata(&job.output) {
                    Ok(metadata) => {
                        ensure!(
                            metadata.is_file() && !metadata.file_type().is_symlink(),
                            "Destination is not a regular file"
                        );
                        fs::remove_file(&job.output).context("Cannot delete downloaded file")?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            } else {
                ensure!(
                    fs::symlink_metadata(&job.output)
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                    "An unfinished job does not own the existing destination; it was preserved"
                );
                crate::state::discard_partial(&job.output, &job.url)?;
            }
            let job = document.job_mut(id)?;
            job.removed = true;
            job.updated_at = timestamp()?;
            Ok(())
        })
    }
}
