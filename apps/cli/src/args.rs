use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use rustdm_core::{DownloadOptions, Sha256Checksum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Rust Download Manager: downloads, a persistent queue, and history",
    subcommand_negates_reqs = true
)]
pub(crate) struct Cli {
    /// Queue/history directory (default: .rustdm in the current directory)
    #[arg(long, global = true, default_value = ".rustdm")]
    pub(crate) state_dir: PathBuf,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    /// Legacy direct-download syntax: rustdm URL -o FILE
    #[command(flatten)]
    pub(crate) direct: DownloadArgs,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Download one file immediately (not added to queue history)
    Download(DownloadArgs),
    /// Manage persistent queued downloads
    Queue {
        #[command(subcommand)]
        action: QueueCommand,
    },
    /// Show terminal attempts from queued downloads
    History {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum QueueCommand {
    /// Add a job; authentication headers cannot be saved in the queue
    Add(DownloadArgs),
    /// Show all jobs and their current state
    List {
        #[arg(long)]
        json: bool,
    },
    /// Run pending/paused jobs sequentially; Ctrl+C pauses the queue
    Run,
    /// Requeue a failed or paused job, preserving its attempt history
    Retry { id: u64 },
}

#[derive(Args)]
pub(crate) struct DownloadArgs {
    /// HTTP or HTTPS download URL
    #[arg(required = true)]
    url: Option<String>,
    /// Destination filename (existing files are never overwritten)
    #[arg(short, long, required = true)]
    output: Option<PathBuf>,
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
    /// Aggregate bytes per second across connections; omit for unlimited speed
    #[arg(long, value_name = "BYTES_PER_SECOND", value_parser = clap::value_parser!(u64).range(1..))]
    limit_rate: Option<u64>,
    /// Expected SHA-256 digest: exactly 64 hexadecimal characters
    #[arg(long, value_name = "HEX")]
    sha256: Option<Sha256Checksum>,
}

impl DownloadArgs {
    pub(crate) fn ensure_no_overrides(&self) -> Result<()> {
        ensure!(
            self.url.is_none()
                && self.output.is_none()
                && self.headers.is_empty()
                && self.limit_rate.is_none()
                && self.sha256.is_none()
                && !self.resume
                && self.connections == 4
                && self.retries == 3,
            "Put download options after the download or queue add command"
        );
        Ok(())
    }

    pub(crate) fn into_options(self) -> Result<DownloadOptions> {
        let mut options = DownloadOptions::new(
            self.url.context("Download URL is required")?,
            self.output.context("Output path is required")?,
        )
        .with_connections(self.connections.into())
        .with_retries(self.retries.into())
        .with_resume(self.resume);
        if let Some(rate) = self.limit_rate {
            options = options.with_bandwidth_limit(rate);
        }
        if let Some(checksum) = self.sha256 {
            options = options.with_sha256(checksum);
        }
        for header in self.headers {
            let (name, value) = header.split_once(':').context("Use -H \"Name: Value\"")?;
            options = options.with_header(name.trim(), value.trim());
        }
        Ok(options)
    }
}

#[cfg(test)]
#[path = "../../../tests/cli/args.rs"]
mod tests;
