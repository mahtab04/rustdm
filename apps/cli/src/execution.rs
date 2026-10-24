use crate::progress::ProgressRenderer;
use anyhow::{Result, bail};
use rustdm_core::{
    CancellationToken, DownloadOptions, DownloadQueue, JobState, Progress, QueueEvent, download,
};
use tokio::{sync::watch, task::JoinHandle};

fn cancellation() -> (CancellationToken, JoinHandle<()>) {
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    (cancel, task)
}

type ProgressSender = watch::Sender<Option<(u64, Progress)>>;

fn reporter() -> (ProgressSender, JoinHandle<()>) {
    let (tx, mut rx) = watch::channel::<Option<(u64, Progress)>>(None);
    let task = tokio::spawn(async move {
        let mut job = None;
        let mut renderer = ProgressRenderer::new();
        while rx.changed().await.is_ok() {
            let current = *rx.borrow_and_update();
            if let Some((id, progress)) = current {
                if job != Some(id) {
                    renderer = ProgressRenderer::new();
                    job = Some(id);
                }
                renderer.render(progress);
            }
        }
    });
    (tx, task)
}

pub(crate) async fn direct(options: DownloadOptions) -> Result<()> {
    let (cancel, signal) = cancellation();
    let (tx, progress) = reporter();
    eprintln!(
        "Downloading to {}. Ctrl+C pauses; range-capable downloads can resume.",
        options.output().display()
    );
    let result = download(&options, cancel, move |value| {
        tx.send_replace(Some((0, value)));
    })
    .await;
    let _ = progress.await;
    signal.abort();
    eprintln!();
    let summary = result?;
    println!(
        "Saved {} ({} bytes, {} connection{}, {} bytes resumed).",
        options.output().display(),
        summary.bytes,
        summary.connections,
        if summary.connections == 1 { "" } else { "s" },
        summary.resumed_bytes
    );
    Ok(())
}

pub(crate) async fn queue(queue: DownloadQueue) -> Result<()> {
    let mut runner = queue.runner()?;
    let (cancel, signal) = cancellation();
    let (tx, progress) = reporter();
    let result = runner
        .run(cancel, move |event| match event {
            QueueEvent::Started { job_id, output } => {
                eprintln!("\nJob {job_id}: {}", output.display())
            }
            QueueEvent::Progress { job_id, progress } => {
                tx.send_replace(Some((job_id, progress)));
            }
            QueueEvent::Connection { .. } => {}
            QueueEvent::Finished {
                job_id,
                state,
                error,
            } => {
                eprintln!("\nJob {job_id}: {state}");
                if state != JobState::Completed
                    && let Some(error) = error
                {
                    eprintln!("{error}");
                }
            }
        })
        .await;
    let _ = progress.await;
    signal.abort();
    eprintln!();
    let result = result?;
    println!(
        "Queue: {} completed, {} failed, {} paused.",
        result.completed, result.failed, result.paused
    );
    if result.cancelled {
        bail!("Queue paused. Run queue run to continue pending/paused jobs");
    }
    if result.failed > 0 {
        bail!("Some jobs failed. Inspect history and use queue retry ID");
    }
    Ok(())
}
