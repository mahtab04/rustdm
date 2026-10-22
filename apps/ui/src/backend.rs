use anyhow::Result;
use eframe::egui;
use rustdm_core::{
    CancellationToken, DownloadOptions, DownloadQueue, HistoryEntry, ProgressMeter, QueueEvent,
    QueueJob, QueueRunSummary, TransferMetrics,
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

pub(crate) enum Event {
    Snapshot(Result<Snapshot, String>),
    Added(Result<bool, String>),
    Changed,
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
        Ok(Self {
            runtime,
            queue,
            sender,
            events,
            progress,
            progress_sender,
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
        let ctx = self.context.clone();
        self.session = Some(self.runtime.spawn(async move {
            let events = sender.clone();
            let repaint = ctx.clone();
            let meter = Mutex::new(ProgressMeter::new());
            let result = async {
                let mut runner = queue.runner()?;
                runner
                    .run(cancel, move |event| match event {
                        QueueEvent::Started { .. } => {
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
                        QueueEvent::Finished { .. } => {
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
    pub(crate) fn pick_destination(&self, filename: String) {
        let sender = self.sender.clone();
        let ctx = self.context.clone();
        self.runtime.spawn_blocking(move || {
            let path = rfd::FileDialog::new()
                .set_title("Save download as")
                .set_file_name(filename)
                .save_file();
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
