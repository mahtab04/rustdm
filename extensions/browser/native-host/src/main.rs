#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod framing;
mod host;
use anyhow::{Context, Result};
use std::{io, path::PathBuf};

fn run() -> Result<()> {
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments == ["--version"] {
        println!("rustdm-browser-host {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if arguments == ["--help"] {
        println!(
            "RustDM native messaging host\nUsage: rustdm-browser-host [--config FILE] BROWSER_CALLER\nBrowser launches read browser-host.json beside the executable. All protocol output is framed JSON."
        );
        return Ok(());
    }
    let config = if arguments.first().is_some_and(|value| value == "--config") {
        anyhow::ensure!(
            arguments.len() >= 3,
            "Missing config path or browser caller"
        );
        let path = PathBuf::from(&arguments[1]);
        arguments.drain(..2);
        path
    } else {
        std::env::current_exe()?
            .parent()
            .context("Missing executable directory")?
            .join("browser-host.json")
    };
    let host = host::Host::new(host::Config::load(&config, &arguments)?);
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(message) = framing::read_message(&mut input)? {
        let response = serde_json::to_vec(&host.handle(&message))?;
        framing::write_message(&mut output, &response)?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("RustDM browser host: {error:#}");
        std::process::exit(1);
    }
}
