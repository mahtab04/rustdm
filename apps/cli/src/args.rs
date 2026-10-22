use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use rustdm_core::DownloadOptions;
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
}

impl DownloadArgs {
    pub(crate) fn ensure_no_overrides(&self) -> Result<()> {
        ensure!(
            self.url.is_none()
                && self.output.is_none()
                && self.headers.is_empty()
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
        for header in self.headers {
            let (name, value) = header.split_once(':').context("Use -H \"Name: Value\"")?;
            options = options.with_header(name.trim(), value.trim());
        }
        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_explicit_download_syntax_both_parse() {
        let legacy =
            Cli::try_parse_from(["rustdm", "https://example.test/a", "-o", "a.bin"]).unwrap();
        assert!(legacy.command.is_none());
        assert_eq!(
            legacy.direct.into_options().unwrap().url(),
            "https://example.test/a"
        );
        let explicit = Cli::try_parse_from([
            "rustdm",
            "download",
            "https://example.test/a",
            "-o",
            "a.bin",
        ])
        .unwrap();
        assert!(matches!(explicit.command, Some(Command::Download(_))));
    }

    #[test]
    fn state_directory_is_global_and_queue_commands_need_no_download_arguments() {
        for args in [
            vec!["rustdm", "--state-dir", "jobs", "queue", "list", "--json"],
            vec!["rustdm", "queue", "list", "--state-dir", "jobs", "--json"],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.state_dir, PathBuf::from("jobs"));
            assert!(matches!(
                cli.command,
                Some(Command::Queue {
                    action: QueueCommand::List { json: true }
                })
            ));
        }
        assert!(Cli::try_parse_from(["rustdm", "history", "--json"]).is_ok());
        assert!(Cli::try_parse_from(["rustdm", "download"]).is_err());
    }
}
