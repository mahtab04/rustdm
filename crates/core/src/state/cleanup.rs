use super::{Manifest, state_directory};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    path::Path,
};

/// Delete only verified sidecar files, never recursively traversing a directory.
pub(crate) fn discard_partial(output: &Path, url: &str) -> Result<()> {
    let directory = state_directory(output);
    let metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Partial directory is not a regular directory"
    );
    let resolved = fs::canonicalize(&directory)?;
    let parent = fs::canonicalize(output.parent().context("Missing destination parent")?)?;
    ensure!(
        resolved.parent() == Some(parent.as_path()),
        "Partial directory leaves the destination folder"
    );
    let lock_path = directory.join("lock");
    if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Partial lock is not a regular file"
        );
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    FileExt::try_lock_exclusive(&lock).context("Another download is using these partial files")?;
    let manifest_path = directory.join("manifest.json");
    let mut expected = HashSet::from(["lock".to_owned()]);
    match fs::symlink_metadata(&manifest_path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Partial manifest is not a regular file"
            );
            let manifest: Manifest = serde_json::from_reader(File::open(&manifest_path)?)?;
            ensure!(
                manifest.version == 1 && manifest.url == url,
                "Partial files belong to another source or format"
            );
            ensure!(
                manifest.segments.len() <= crate::domain::MAX_SEGMENTS,
                "Invalid partial segment count"
            );
            let count = if manifest.segmented {
                ensure!(!manifest.segments.is_empty(), "Missing partial segments");
                manifest.segments.len()
            } else {
                1
            };
            expected.insert("manifest.json".into());
            for index in 0..count {
                expected.insert(format!("part-{index:03}"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let entries = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
    for entry in &entries {
        let kind = entry.file_type()?;
        ensure!(
            kind.is_file()
                && !kind.is_symlink()
                && expected.contains(&entry.file_name().to_string_lossy().to_string()),
            "Unrecognized files exist in the sidecar; all files were preserved"
        );
    }
    for entry in entries {
        if entry.file_name() != "lock" {
            fs::remove_file(entry.path())?;
        }
    }
    // Keep the locked inode and empty sidecar so competing writers cannot bypass
    // an unlinked lock. No downloaded data remains; future downloads reuse it.
    Ok(())
}
