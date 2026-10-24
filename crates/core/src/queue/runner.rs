use super::{DownloadQueue, JobState, QueueEvent, QueueRunSummary};
use crate::{CancellationToken, ConnectionProgress, DownloadManager, Progress, ProgressObserver};
use anyhow::Result;
use std::{fs::File, sync::Arc};

/// An exclusive queue execution session. Dropping it releases the OS runner lock.
pub struct QueueRunner {
    queue: DownloadQueue,
    _guard: File,
}

impl DownloadQueue {
    pub fn runner(&self) -> Result<QueueRunner> {
        let guard = self.runner_lock()?;
        self.recover_interrupted()?;
        Ok(QueueRunner {
            queue: self.clone(),
            _guard: guard,
        })
    }
}

impl QueueRunner {
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
        let observer: Arc<dyn Fn(QueueEvent) + Send + Sync> = Arc::new(observer);
        let mut result = QueueRunSummary::default();
        loop {
            if cancel.is_cancelled() {
                result.cancelled = true;
                break;
            }
            let Some(job) = self.queue.claim_next()? else {
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
                .download(cancel.child_token(), progress)
                .await;
            // Successful publication wins over a cancellation arriving just after
            // completion; the job must not be retried over its completed output.
            let (state, summary, error) = match download {
                Ok(summary) => {
                    result.completed += 1;
                    (JobState::Completed, Some(summary), None)
                }
                Err(error) if cancel.is_cancelled() => {
                    result.paused += 1;
                    result.cancelled = true;
                    (JobState::Paused, None, Some(format!("{error:#}")))
                }
                Err(error) => {
                    result.failed += 1;
                    (JobState::Failed, None, Some(format!("{error:#}")))
                }
            };
            self.queue
                .finish(id, state, summary.as_ref(), error.clone())?;
            observer(QueueEvent::Finished {
                job_id: id,
                state,
                error,
            });
            if result.cancelled {
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
