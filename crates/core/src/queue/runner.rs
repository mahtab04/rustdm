use super::{DownloadQueue, JobState, QueueEvent, QueueRunSummary};
use crate::{CancellationToken, ConnectionProgress, DownloadManager, Progress, ProgressObserver};
use anyhow::{Context, Result};
use std::{
    fs::File,
    sync::{Arc, Mutex, Weak},
};

/// An exclusive queue execution session. Dropping it releases the OS runner lock.
pub struct QueueRunner {
    queue: DownloadQueue,
    guard: Arc<File>,
    active: Arc<Mutex<Option<(u64, CancellationToken)>>>,
}

/// Controls only the execution session that created it.
#[derive(Clone)]
pub struct QueueControl {
    queue: DownloadQueue,
    guard: Weak<File>,
    active: Arc<Mutex<Option<(u64, CancellationToken)>>>,
}

impl QueueControl {
    /// Hold a job and cancel its active transfer, leaving the queue free to continue.
    pub fn pause(&self, id: u64) -> Result<()> {
        let _guard = self
            .guard
            .upgrade()
            .context("Queue execution session has ended")?;
        let active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Queue controller failed"))?;
        let current = active.as_ref().filter(|(active_id, _)| *active_id == id);
        self.queue.hold(id, current.is_some())?;
        if let Some((_, cancel)) = current {
            cancel.cancel();
        }
        Ok(())
    }
}

impl DownloadQueue {
    pub fn runner(&self) -> Result<QueueRunner> {
        let guard = self.runner_lock()?;
        self.recover_interrupted()?;
        Ok(QueueRunner {
            queue: self.clone(),
            guard: Arc::new(guard),
            active: Arc::new(Mutex::new(None)),
        })
    }
}

impl QueueRunner {
    pub fn control(&self) -> QueueControl {
        QueueControl {
            queue: self.queue.clone(),
            guard: Arc::downgrade(&self.guard),
            active: self.active.clone(),
        }
    }

    /// Run only the selected pending/paused/failed job, including an explicitly held job.
    pub async fn run_job<O>(
        &mut self,
        id: u64,
        cancel: CancellationToken,
        observer: O,
    ) -> Result<QueueRunSummary>
    where
        O: Fn(QueueEvent) + Send + Sync + 'static,
    {
        self.run_selected(Some(id), cancel, observer).await
    }

    /// Runs pending/paused jobs sequentially. Download failures are recorded and
    /// do not stop later jobs. Storage errors stop execution to preserve history.
    pub async fn run<O>(
        &mut self,
        cancel: CancellationToken,
        observer: O,
    ) -> Result<QueueRunSummary>
    where
        O: Fn(QueueEvent) + Send + Sync + 'static,
    {
        self.run_selected(None, cancel, observer).await
    }

    async fn run_selected<O>(
        &mut self,
        selected: Option<u64>,
        cancel: CancellationToken,
        observer: O,
    ) -> Result<QueueRunSummary>
    where
        O: Fn(QueueEvent) + Send + Sync + 'static,
    {
        let observer: Arc<dyn Fn(QueueEvent) + Send + Sync> = Arc::new(observer);
        let mut result = QueueRunSummary::default();
        loop {
            if cancel.is_cancelled() {
                result.cancelled = true;
                break;
            }
            let job_cancel = cancel.child_token();
            let job = {
                let mut active = self
                    .active
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Queue controller failed"))?;
                let job = match selected {
                    Some(_) => self.queue.claim(selected)?,
                    None => self.queue.claim_next()?,
                };
                if let Some(job) = &job {
                    *active = Some((job.id(), job_cancel.clone()));
                }
                job
            };
            let Some(job) = job else {
                break;
            };
            observer(QueueEvent::Started {
                job_id: job.id(),
                output: job.output().to_path_buf(),
            });
            let id = job.id();
            let progress = QueueObserver {
                id,
                callback: observer.clone(),
            };
            let download = DownloadManager::new(job.options())
                .download(job_cancel.clone(), progress)
                .await;
            // Successful publication wins over a cancellation arriving just after
            // completion; the job must not be retried over its completed output.
            let (state, summary, error) = match download {
                Ok(summary) => {
                    result.completed += 1;
                    (JobState::Completed, Some(summary), None)
                }
                Err(error) if job_cancel.is_cancelled() => {
                    result.paused += 1;
                    result.cancelled = cancel.is_cancelled();
                    (JobState::Paused, None, Some(format!("{error:#}")))
                }
                Err(error) => {
                    result.failed += 1;
                    (JobState::Failed, None, Some(format!("{error:#}")))
                }
            };
            {
                let mut active = self
                    .active
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Queue controller failed"))?;
                self.queue
                    .finish(id, state, summary.as_ref(), error.clone())?;
                *active = None;
            }
            observer(QueueEvent::Finished {
                job_id: id,
                state,
                error,
            });
            if result.cancelled || selected.is_some() {
                break;
            }
        }
        Ok(result)
    }
}

struct QueueObserver {
    id: u64,
    callback: Arc<dyn Fn(QueueEvent) + Send + Sync>,
}
impl ProgressObserver for QueueObserver {
    fn on_progress(&self, progress: Progress) {
        (self.callback)(QueueEvent::Progress {
            job_id: self.id,
            progress,
        });
    }
    fn on_connection(&self, progress: ConnectionProgress) {
        (self.callback)(QueueEvent::Connection {
            job_id: self.id,
            progress,
        });
    }
}
