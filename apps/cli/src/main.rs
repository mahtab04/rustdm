mod progress;

use anyhow::{Context, Result};
use clap::Parser;
use progress::ProgressRenderer;
use rustdm_core::{CancellationToken, DownloadOptions, Progress, download};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Rust Download Manager: parallel HTTP downloads with validated resume"
)]
struct Args {
    /// HTTP or HTTPS download URL
    url: String,
    /// Destination filename (existing files are never overwritten)
    #[arg(short, long)]
    output: PathBuf,
    /// Parallel connections, from 1 to 16
    #[arg(short = 'c', long, default_value_t = 4, value_parser = clap::value_parser!(u8).range(1..=16))]
    connections: u8,
    /// Continue a saved partial download
    #[arg(long)]
    resume: bool,
    /// Additional attempts for a failed connection, from 0 to 10
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=10))]
    retries: u8,
    /// Request header; repeat for cookies, referrer, authorization, etc.
    #[arg(short = 'H', long = "header", value_name = "NAME: VALUE")]
    headers: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut options = DownloadOptions::new(args.url, args.output)
        .with_connections(args.connections.into())
        .with_retries(args.retries.into())
        .with_resume(args.resume);
    for header in args.headers {
        let (name, value) = header.split_once(':').context("Use -H \"Name: Value\"")?;
        options = options.with_header(name.trim(), value.trim());
    }
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let signal_task = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    // A watch channel coalesces rapid worker events and retains the latest update,
    // including completion, without allowing terminal output to block transfers.
    let (tx, mut rx) = tokio::sync::watch::channel::<Option<Progress>>(None);
    let reporter = tokio::spawn(async move {
        let mut renderer = ProgressRenderer::new();
        while rx.changed().await.is_ok() {
            let current = *rx.borrow_and_update();
            if let Some(progress) = current {
                renderer.render(progress);
            }
        }
    });
    eprintln!(
        "Downloading to {}. Ctrl+C pauses; range-capable downloads can resume.",
        options.output().display()
    );
    let result = download(&options, cancel, move |progress| {
        tx.send_replace(Some(progress));
    })
    .await;
    let _ = reporter.await;
    signal_task.abort();
    eprintln!();
    match result {
        Ok(summary) => {
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
        Err(error) => Err(error),
    }
}
