use anyhow::Result;
use eframe::egui;
use rustdm_core::{
    CancellationToken, ConnectionProgress, DownloadOptions, DownloadQueue, HistoryEntry,
    ProgressMeter, QueueEvent, QueueJob, QueueRunSummary, TransferMetrics,
};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
};
use tokio::{runtime::Runtime, sync::watch, task::JoinHandle};

pub(crate) struct Snapshot {
    pub(crate) jobs: Vec<QueueJob>,
    pub(crate) history: Vec<HistoryEntry>,
}

#[derive(Clone)]
pub(crate) struct ConnectionSnapshot {
    pub(crate) job_id: u64,
    pub(crate) workers: Vec<ConnectionProgress>,
}

pub(crate) enum Event {
    Snapshot(Result<Snapshot, String>),
    Added(Result<bool, String>),
    Changed,
    Connections(ConnectionSnapshot),
    Stopped(Result<QueueRunSummary, String>),
    Retried(Result<(), String>),
    Destination(Option<PathBuf>),
}

/// HTTP and persistence stay in the core. Progress is coalesced separately from lifecycle events.
pub(crate) struct Backend {
    runtime: Runtime,
    queue: DownloadQueue,
    sender: mpsc::Sender<Event>,
    pub(crate) events: mpsc::Receiver<Event>,
    pub(crate) progress: watch::Receiver<Option<(u64, TransferMetrics)>>,
    progress_sender: watch::Sender<Option<(u64, TransferMetrics)>>,
    pub(crate) connections: watch::Receiver<Option<ConnectionSnapshot>>,
    connection_sender: watch::Sender<Option<ConnectionSnapshot>>,
    session: Option<JoinHandle<()>>,
    cancel: CancellationToken,
    context: egui::Context,
}

impl Backend {
    pub(crate) fn new(directory: PathBuf, context: egui::Context) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let queue = DownloadQueue::open(directory)?;
        let (sender, events) = mpsc::channel();
        let (progress_sender, progress) = watch::channel(None);
        let (connection_sender, connections) = watch::channel(None);
        Ok(Self {
            runtime,
            queue,
            sender,
            events,
            progress,
            progress_sender,
            connections,
            connection_sender,
            session: None,
            cancel: CancellationToken::new(),
            context,
        })
    }
    pub(crate) fn directory(&self) -> &std::path::Path {
        self.queue.directory()
    }
    pub(crate) fn running(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|task| !task.is_finished())
    }
    pub(crate) fn reap(&mut self) {
        if self.session.as_ref().is_some_and(|task| task.is_finished()) {
            let task = self.session.take().expect("Finished session exists");
            if let Err(error) = self.runtime.block_on(task) {
                let _ = self.sender.send(Event::Stopped(Err(format!(
                    "Download worker stopped unexpectedly: {error}"
                ))));
            }
        }
    }
    pub(crate) fn pause(&self) {
        self.cancel.cancel();
    }
    pub(crate) fn refresh(&self) {
        let queue = self.queue.clone();
        let sender = self.sender.clone();
        let ctx = self.context.clone();
        self.runtime.spawn_blocking(move || {
            let snapshot = (|| {
                Ok(Snapshot {
                    jobs: queue.jobs()?,
                    history: queue.history()?,
                })
            })()
            .map_err(|error: anyhow::Error| format!("{error:#}"));
            let _ = sender.send(Event::Snapshot(snapshot));
            ctx.request_repaint();
        });
    }
    pub(crate) fn add(&self, options: DownloadOptions, start: bool) {
        let queue = self.queue.clone();
        let sender = self.sender.clone();
        let ctx = self.context.clone();
        self.runtime.spawn_blocking(move || {
            let result = queue
                .enqueue(options)
                .map(|_| start)
                .map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Added(result));
            ctx.request_repaint();
        });
    }
    pub(crate) fn retry(&self, id: u64) {
        let queue = self.queue.clone();
        let sender = self.sender.clone();
        let ctx = self.context.clone();
        self.runtime.spawn_blocking(move || {
            let result = queue.retry(id).map_err(|e| format!("{e:#}"));
            let _ = sender.send(Event::Retried(result));
            ctx.request_repaint();
        });
    }
    pub(crate) fn start(&mut self) {
        if self.running() {
            return;
        }
        self.cancel = CancellationToken::new();
        let cancel = self.cancel.clone();
        let queue = self.queue.clone();
        let sender = self.sender.clone();
        let progress = self.progress_sender.clone();
        let connections = self.connection_sender.clone();
        let ctx = self.context.clone();
        self.session = Some(self.runtime.spawn(async move {
            let events = sender.clone();
            let repaint = ctx.clone();
            let meter = Mutex::new(ProgressMeter::new());
            let result = async {
                let mut runner = queue.runner()?;
                runner
                    .run(cancel, move |event| match event {
                        QueueEvent::Started { job_id, .. } => {
                            connections.send_replace(Some(ConnectionSnapshot {
                                job_id,
                                workers: Vec::new(),
                            }));
                            *meter.lock().expect("Progress meter poisoned") = ProgressMeter::new();
                            progress.send_replace(None);
                            let _ = events.send(Event::Changed);
                            repaint.request_repaint();
                        }
                        QueueEvent::Progress {
                            job_id,
                            progress: value,
                        } => {
                            let metrics = meter
                                .lock()
                                .expect("Progress meter poisoned")
                                .observe(value);
                            progress.send_replace(Some((job_id, metrics)));
                        }
                        QueueEvent::Connection { job_id, progress } => {
                            connections.send_modify(|snapshot| {
                                if snapshot.as_ref().is_none_or(|s| s.job_id != job_id) {
                                    *snapshot = Some(ConnectionSnapshot {
                                        job_id,
                                        workers: Vec::new(),
                                    });
                                }
                                let workers = &mut snapshot
                                    .as_mut()
                                    .expect("Connection snapshot initialized")
                                    .workers;
                                if let Some(worker) =
                                    workers.iter_mut().find(|worker| worker.id == progress.id)
                                {
                                    *worker = progress;
                                } else {
                                    workers.push(progress);
                                    workers.sort_by_key(|worker| worker.id);
                                }
                            });
                        }
                        QueueEvent::Finished { .. } => {
                            if let Some(snapshot) = connections.borrow().clone() {
                                let _ = events.send(Event::Connections(snapshot));
                            }
                            let _ = events.send(Event::Changed);
                            repaint.request_repaint();
                        }
                    })
                    .await
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"));
            let _ = sender.send(Event::Stopped(result));
            ctx.request_repaint();
        }));
    }
    pub(crate) fn pick_destination(&self, filename: String, directory: Option<PathBuf>) {
        let sender = self.sender.clone();
        let ctx = self.context.clone();
        self.runtime.spawn_blocking(move || {
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save download as")
                .set_file_name(filename);
            if let Some(directory) = directory.filter(|path| path.is_dir()) {
                dialog = dialog.set_directory(directory);
            }
            let path = dialog.save_file();
            let _ = sender.send(Event::Destination(path));
            ctx.request_repaint();
        });
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/backend.rs"]
mod tests;
