use anyhow::{Context, Result};
use clap::Parser;
use rustidm::{CancellationToken, DownloadOptions, Progress, download};
use std::{
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    version,
    about = "Download files using parallel HTTP ranges with recoverable partial files"
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

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut options = DownloadOptions::new(args.url, args.output);
    options = options.with_connections(args.connections.into());
    options = options.with_retries(args.retries.into());
    options = options.with_resume(args.resume);
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
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Progress>(64);
    let reporter = tokio::spawn(async move {
        let mut previous = Instant::now();
        while let Some(progress) = rx.recv().await {
            if previous.elapsed() < Duration::from_millis(200)
                && progress.total != Some(progress.downloaded)
            {
                continue;
            }
            previous = Instant::now();
            match progress.total {
                Some(total) => eprint!(
                    "\r{} / {} ({:.1}%)     ",
                    format_size(progress.downloaded),
                    format_size(total),
                    if total == 0 {
                        100.0
                    } else {
                        progress.downloaded as f64 * 100.0 / total as f64
                    }
                ),
                None => eprint!("\r{} downloaded     ", format_size(progress.downloaded)),
            }
            let _ = io::stderr().flush();
        }
    });
    eprintln!(
        "Downloading to {}. Ctrl+C pauses; range-capable downloads can resume.",
        options.output().display()
    );
    let result = download(&options, cancel, move |progress| {
        let _ = tx.try_send(progress);
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
