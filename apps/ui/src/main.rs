#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod app;
mod backend;
mod model;
mod theme;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "RustDM desktop download manager")]
struct Args {
    /// Use a persistent queue directory, including one shared with the CLI
    #[arg(long)]
    state_dir: Option<PathBuf>,
}

fn default_state() -> PathBuf {
    if let Some(directory) =
        std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("XDG_DATA_HOME"))
    {
        return PathBuf::from(directory).join("RustDM");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/RustDM")
}

fn main() -> eframe::Result {
    let directory = Args::parse().state_dir.unwrap_or_else(default_state);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([940.0, 620.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "RustDM",
        options,
        Box::new(move |cc| Ok(Box::new(app::DesktopApp::new(cc, directory)?))),
    )
}
