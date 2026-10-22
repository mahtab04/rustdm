use crate::{
    CancellationToken,
    domain::{DownloadOptions, Remote, Segment, Validator},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Manifest {
    pub(crate) version: u32,
    pub(crate) url: String,
    pub(crate) final_url: String,
    pub(crate) size: Option<u64>,
    pub(crate) validator: Option<Validator>,
    pub(crate) segmented: bool,
    pub(crate) segments: Vec<Segment>,
}

impl Manifest {
    fn validate(&self, options: &DownloadOptions, remote: &Remote) -> Result<()> {
        ensure!(self.version == 1, "Unsupported partial-download format");
        ensure!(
            self.url == options.url && self.final_url == remote.final_url,
            "Resume URL differs from the saved download"
        );
        ensure!(
            self.segmented && remote.ranges && self.validator.is_some(),
            "This download cannot safely resume: byte ranges and a server validator are required. Use a new output path"
        );
        ensure!(
            self.size == remote.size && self.validator == remote.validator,
            "Remote file changed; refusing to combine it with saved data. Use a new output path"
        );
        let size = self.size.context("Missing saved total")?;
        ensure!(
            !self.segments.is_empty() && self.segments.len() <= 16,
            "Invalid saved segment count"
        );
        let mut next = 0;
        for segment in &self.segments {
            ensure!(
                segment.start == next && segment.end > segment.start && segment.end <= size,
                "Invalid saved segment layout"
            );
            next = segment.end;
        }
        ensure!(next == size, "Saved segments do not cover the entire file");
        Ok(())
    }
}
/// Location of the recoverable partial files. No authentication headers are stored.
pub fn state_directory(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".rustidm");
    PathBuf::from(name)
}

pub(crate) fn part_size(path: &Path) -> Result<u64> {
    match fs::metadata(path) {
        Ok(metadata) => {
            ensure!(metadata.is_file(), "Partial path is not a regular file");
            Ok(metadata.len())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

/// Owns the sidecar directory and its exclusive writer lock.
pub(crate) struct DownloadState {
    directory: PathBuf,
    manifest_path: PathBuf,
    lock: File,
}
impl DownloadState {
    pub(crate) fn acquire(output: &Path, resume: bool) -> Result<Self> {
        ensure!(
            !output.exists(),
            "Output already exists; choose a different filename"
        );
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).context("Cannot create output directory")?;
        let state = state_directory(output);
        fs::create_dir_all(&state)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(state.join("lock"))?;
        FileExt::try_lock_exclusive(&lock).context("Another download is using this output")?;
        ensure!(!output.exists(), "Output was created by another download");
        let manifest_path = state.join("manifest.json");
        ensure!(
            !manifest_path.exists() || resume,
            "Partial download exists. Run again with --resume"
        );
        Ok(Self {
            directory: state,
            manifest_path,
            lock,
        })
    }
    pub(crate) fn part_path(&self, index: usize) -> PathBuf {
        self.directory.join(format!("part-{index:03}"))
    }
    pub(crate) fn load_or_create(
        &self,
        options: &DownloadOptions,
        remote: &Remote,
    ) -> Result<Manifest> {
        let segmented =
            remote.ranges && remote.validator.is_some() && remote.size.is_some_and(|n| n > 0);
        let manifest = if self.manifest_path.exists() {
            let saved: Manifest = serde_json::from_reader(File::open(&self.manifest_path)?)
                .context("Cannot read saved download manifest")?;
            saved.validate(options, remote)?;
            saved
        } else {
            ensure!(
                fs::read_dir(&self.directory)?
                    .all(|entry| entry.is_ok_and(|entry| entry.file_name() == "lock")),
                "Unrecognized files exist in the download sidecar; use a new output path"
            );
            let manifest = Manifest {
                version: 1,
                url: options.url.clone(),
                final_url: remote.final_url.clone(),
                size: remote.size,
                validator: remote.validator.clone(),
                segmented,
                segments: if segmented {
                    Segment::plan(remote.size.unwrap(), options.connections)
                } else {
                    Vec::new()
                },
            };
            let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
            serde_json::to_writer_pretty(&mut temp, &manifest)?;
            temp.as_file().sync_all()?;
            temp.persist_noclobber(&self.manifest_path)
                .context("Cannot save download manifest")?;
            manifest
        };
        Ok(manifest)
    }
    pub(crate) async fn publish(
        &self,
        output: &Path,
        manifest: &Manifest,
        part_count: usize,
        bytes: u64,
        cancel: &CancellationToken,
    ) -> Result<()> {
        ensure!(
            !cancel.is_cancelled(),
            "Download paused before assembly; partial files retained"
        );
        let mut assembled = tempfile::NamedTempFile::new_in(&self.directory)?;
        let mut copied = 0;
        let mut buffer = [0u8; 64 * 1024];
        for index in 0..part_count {
            let path = self.part_path(index);
            let mut part = File::open(&path)?;
            if manifest.segmented {
                ensure!(
                    part.metadata()?.len()
                        == manifest.segments[index].end - manifest.segments[index].start,
                    "Part size changed before assembly"
                );
            }
            loop {
                ensure!(
                    !cancel.is_cancelled(),
                    "Download paused during assembly; partial files retained"
                );
                let count = part.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                assembled.write_all(&buffer[..count])?;
                copied += count as u64;
                tokio::task::yield_now().await;
            }
        }
        ensure!(
            copied == bytes,
            "Assembled size does not match downloaded size"
        );
        assembled.as_file().sync_all()?;
        assembled
            .persist_noclobber(output)
            .context("Cannot publish output without overwriting an existing file")?;
        Ok(())
    }
    pub(crate) fn cleanup(self, part_count: usize) {
        // Remove only files owned by this download, never arbitrary sidecar contents.
        for index in 0..part_count {
            let _ = fs::remove_file(self.part_path(index));
        }
        let _ = fs::remove_file(&self.manifest_path);
        drop(self.lock);
        let _ = fs::remove_file(self.directory.join("lock"));
        let _ = fs::remove_dir(&self.directory);
    }
}
