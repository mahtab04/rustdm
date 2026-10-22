mod args;
mod execution;
mod progress;

use anyhow::Result;
use args::{Cli, Command, QueueCommand};
use clap::Parser;
use rustdm_core::DownloadQueue;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.command.is_some() {
        cli.direct.ensure_no_overrides()?;
    }
    match cli.command.unwrap_or(Command::Download(cli.direct)) {
        Command::Download(args) => execution::direct(args.into_options()?).await,
        Command::Queue { action } => {
            let queue = DownloadQueue::open(cli.state_dir)?;
            match action {
                QueueCommand::Add(args) => {
                    println!("Queued job {}", queue.enqueue(args.into_options()?)?);
                    Ok(())
                }
                QueueCommand::List { json } => {
                    let jobs = queue.jobs()?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&jobs)?);
                    } else {
                        println!("ID\tSTATUS\tATTEMPTS\tOUTPUT");
                        for job in jobs {
                            println!(
                                "{}\t{}\t{}\t{}",
                                job.id(),
                                job.state(),
                                job.attempt_count(),
                                job.output().display()
                            );
                        }
                    }
                    Ok(())
                }
                QueueCommand::Run => execution::queue(queue).await,
                QueueCommand::Retry { id } => {
                    queue.retry(id)?;
                    println!("Requeued job {id}");
                    Ok(())
                }
            }
        }
        Command::History { json } => {
            let history = DownloadQueue::open(cli.state_dir)?.history()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&history)?);
            } else {
                println!("JOB\tATTEMPT\tSTATUS\tBYTES\tOUTPUT");
                for entry in history {
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        entry.job_id,
                        entry.attempt,
                        entry.state,
                        entry
                            .bytes
                            .map_or_else(|| "-".into(), |bytes| bytes.to_string()),
                        entry.output.display()
                    );
                }
            }
            Ok(())
        }
    }
}
