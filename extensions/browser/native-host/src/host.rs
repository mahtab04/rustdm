use anyhow::{Context, Result, ensure};
use rustdm_core::{DownloadOptions, DownloadQueue};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    version: u32,
    state_dir: PathBuf,
    download_directory: PathBuf,
    allowed_callers: Vec<String>,
}

impl Config {
    pub(crate) fn load(path: &Path, arguments: &[String]) -> Result<Self> {
        let config: Self =
            serde_json::from_slice(&fs::read(path).context("Read browser-host.json")?)?;
        ensure!(
            config.version == 1,
            "Unsupported browser host configuration"
        );
        ensure!(
            config.state_dir.is_absolute() && config.download_directory.is_absolute(),
            "Host directories must be absolute"
        );
        ensure!(
            config.download_directory.is_dir(),
            "Configured download folder does not exist"
        );
        // Chromium supplies its origin; Firefox supplies manifest path then add-on ID.
        let caller = match arguments {
            [origin] if valid_chromium_origin(origin) => origin,
            [origin, parent]
                if valid_chromium_origin(origin)
                    && parent
                        .strip_prefix("--parent-window=")
                        .is_some_and(|value| {
                            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                        }) =>
            {
                origin
            }
            [manifest, addon]
                if Path::new(manifest).is_absolute() && addon == "rustdm@mahtab04" =>
            {
                addon
            }
            _ => anyhow::bail!("Unrecognized browser caller"),
        };
        ensure!(
            config.allowed_callers.contains(caller),
            "Browser extension is not allowed"
        );
        Ok(config)
    }
}

fn valid_chromium_origin(value: &str) -> bool {
    value
        .strip_prefix("chrome-extension://")
        .and_then(|v| v.strip_suffix('/'))
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    id: String,
    action: String,
    url: String,
    filename: String,
}

#[derive(Serialize)]
pub(crate) struct Response {
    version: u32,
    id: Option<String>,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub(crate) struct Host {
    config: Config,
}

impl Host {
    pub(crate) fn new(config: Config) -> Self {
        Self { config }
    }

    pub(crate) fn handle(&self, message: &[u8]) -> Response {
        let mut id = None;
        let result = (|| -> Result<(u64, PathBuf)> {
            let request: Request =
                serde_json::from_slice(message).context("Invalid request schema")?;
            ensure!(
                !request.id.is_empty() && request.id.len() <= 80,
                "Invalid request ID"
            );
            id = Some(request.id);
            ensure!(
                request.version == 1 && request.action == "enqueue",
                "Unsupported request version or action"
            );
            ensure!(request.url.len() <= 16 * 1024, "URL is too long");
            validate_filename(&request.filename)?;
            // Match the CLI/desktop queue's absolute-path representation. Canonicalizing
            // here adds a Windows verbatim prefix that can disguise duplicate outputs.
            let directory = std::path::absolute(&self.config.download_directory)?;
            let output = directory.join(request.filename);
            match fs::symlink_metadata(&output) {
                Ok(_) => anyhow::bail!("Destination already exists; choose another filename"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error).context("Cannot inspect destination"),
            }
            let queue = DownloadQueue::open(&self.config.state_dir)?;
            let job_id = queue.enqueue(DownloadOptions::new(request.url, &output))?;
            Ok((job_id, output))
        })();
        match result {
            Ok((job_id, output)) => Response {
                version: 1,
                id,
                ok: true,
                job_id: Some(job_id),
                output: Some(output),
                error: None,
            },
            Err(error) => Response {
                version: 1,
                id,
                ok: false,
                job_id: None,
                output: None,
                error: Some(format!("{error:#}")),
            },
        }
    }
}

fn validate_filename(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 180 && value != "." && value != "..",
        "Filename must be 1 to 180 UTF-8 bytes"
    );
    ensure!(
        !value.ends_with(['.', ' '])
            && !value
                .chars()
                .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)),
        "Filename contains unsupported characters or a path"
    );
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_uppercase();
    ensure!(
        !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !["COM", "LPT"].iter().any(|prefix| {
                stem.strip_prefix(prefix).is_some_and(|suffix| {
                    matches!(
                        suffix,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                })
            }),
        "Filename is reserved by Windows"
    );
    Ok(())
}
