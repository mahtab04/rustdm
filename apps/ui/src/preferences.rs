use crate::model::DownloadForm;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    version: u32,
    pub(crate) download_directory: String,
    pub(crate) connections: usize,
    pub(crate) limit_kib: Option<u64>,
    pub(crate) resume: bool,
    pub(crate) close_to_tray: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            download_directory: String::new(),
            connections: 4,
            limit_kib: None,
            resume: false,
            close_to_tray: false,
        }
    }
}

impl Preferences {
    fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported preferences version");
        ensure!(
            (1..=16).contains(&self.connections),
            "Connections must be 1 to 16"
        );
        if let Some(rate) = self.limit_kib {
            ensure!(
                rate > 0 && rate.checked_mul(1024).is_some(),
                "Invalid speed limit"
            );
        }
        Ok(())
    }

    pub(crate) fn form(&self) -> DownloadForm {
        DownloadForm {
            connections: self.connections,
            rate: self
                .limit_kib
                .map(|rate| rate.to_string())
                .unwrap_or_default(),
            resume: self.resume,
            ..DownloadForm::default()
        }
    }
}

pub(crate) struct PreferencesStore {
    directory: PathBuf,
}

impl PreferencesStore {
    pub(crate) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub(crate) fn load(&self) -> Result<Preferences> {
        let data = match fs::read(self.directory.join("preferences.json")) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Preferences::default());
            }
            Err(error) => return Err(error).context("Cannot read desktop preferences"),
        };
        let preferences: Preferences = serde_json::from_slice(&data)
            .context("Cannot parse desktop preferences; existing file was preserved")?;
        preferences.validate()?;
        Ok(preferences)
    }

    pub(crate) fn save(&self, preferences: &Preferences) -> Result<()> {
        preferences.validate()?;
        fs::create_dir_all(&self.directory)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.directory.join("preferences.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("Settings are being saved by another window; try again")?;
        // Preserve malformed files and formats written by newer applications.
        self.load()?;
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer_pretty(&mut temporary, preferences)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.directory.join("preferences.json"))?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/ui/preferences.rs"]
mod tests;
