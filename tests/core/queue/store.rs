use super::{DownloadQueue, JobState};
use crate::DownloadOptions;
use std::{collections::HashSet, fs};

fn options(output: impl Into<std::path::PathBuf>) -> DownloadOptions {
    DownloadOptions::new("https://example.test/archive.zip", output)
}

#[test]
fn individual_holds_persist_and_selected_claims_skip_other_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let first = queue.enqueue(options(temp.path().join("first"))).unwrap();
    let second = queue.enqueue(options(temp.path().join("second"))).unwrap();
    queue.pause(first).unwrap();
    let reopened = DownloadQueue::open(queue.directory()).unwrap();
    assert!(reopened.jobs().unwrap()[0].is_held());
    assert_eq!(reopened.jobs().unwrap()[0].state(), JobState::Paused);
    let runner = reopened.runner().unwrap();
    assert_eq!(reopened.claim_next().unwrap().unwrap().id(), second);
    assert!(queue.remove(second).is_err());
    assert!(queue.delete_files(second).is_err());
    reopened
        .finish(second, JobState::Failed, None, Some("test".into()))
        .unwrap();
    assert!(reopened.claim_next().unwrap().is_none());
    let selected = reopened.claim(Some(first)).unwrap().unwrap();
    assert_eq!(selected.id(), first);
    assert!(!selected.is_held());
    assert!(
        queue.pause(first).is_err(),
        "A store cannot cancel another session's active job"
    );
    runner.control().pause(first).unwrap_err(); // claim was made outside the controlled execution path
    drop(runner);
}

#[test]
fn removing_a_job_keeps_files_and_history_and_never_reuses_its_id() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("kept.bin");
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let id = queue.enqueue(options(&output)).unwrap();
    let _runner = queue.runner().unwrap();
    queue.claim_next().unwrap();
    fs::write(&output, b"keep this file").unwrap();
    queue.finish(id, JobState::Completed, None, None).unwrap();
    queue.remove(id).unwrap();
    assert!(queue.jobs().unwrap().is_empty());
    assert_eq!(queue.history().unwrap()[0].job_id, id);
    assert_eq!(fs::read(&output).unwrap(), b"keep this file");
    assert!(queue.enqueue(options(temp.path().join("next"))).unwrap() > id);
}

fn partial_fixture(output: &std::path::Path, url: &str) -> std::path::PathBuf {
    let directory = crate::state_directory(output);
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("manifest.json"), serde_json::to_vec(&serde_json::json!({
        "version":1,"url":url,"final_url":url,"size":3,"validator":null,"segmented":false,"segments":[]
    })).unwrap()).unwrap();
    fs::write(directory.join("part-000"), b"abc").unwrap();
    directory
}

#[test]
fn deleting_partial_data_requires_owned_regular_files_and_an_exclusive_writer_lock() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("partial.bin");
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let id = queue.enqueue(options(&output)).unwrap();
    let directory = partial_fixture(&output, "https://example.test/archive.zip");
    fs::write(directory.join("unrelated.txt"), b"preserve").unwrap();
    assert!(queue.delete_files(id).is_err());
    assert_eq!(fs::read(directory.join("part-000")).unwrap(), b"abc");
    assert!(directory.join("manifest.json").exists());
    assert_eq!(
        fs::read(directory.join("unrelated.txt")).unwrap(),
        b"preserve"
    );
    fs::remove_file(directory.join("unrelated.txt")).unwrap();
    let writer = crate::state::DownloadState::acquire(&output, true).unwrap();
    assert!(queue.delete_files(id).is_err());
    drop(writer);
    queue.delete_files(id).unwrap();
    assert!(queue.jobs().unwrap().is_empty());
    assert!(!directory.join("part-000").exists());
    assert!(!directory.join("manifest.json").exists());
    assert!(directory.join("lock").exists());
}

#[test]
fn deleting_a_completed_file_retains_history_and_unfinished_jobs_preserve_existing_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let completed = temp.path().join("completed.bin");
    let id = queue.enqueue(options(&completed)).unwrap();
    let _runner = queue.runner().unwrap();
    queue.claim_next().unwrap();
    fs::write(&completed, b"complete").unwrap();
    queue.finish(id, JobState::Completed, None, None).unwrap();
    queue.delete_files(id).unwrap();
    assert!(!completed.exists());
    assert_eq!(queue.history().unwrap().len(), 1);
    let pending = temp.path().join("pending.bin");
    let next = queue.enqueue(options(&pending)).unwrap();
    fs::write(&pending, b"someone else's file").unwrap();
    assert!(queue.delete_files(next).is_err());
    assert_eq!(fs::read(pending).unwrap(), b"someone else's file");
    assert_eq!(queue.jobs().unwrap().len(), 1);
}

#[test]
fn mismatched_partial_sources_are_preserved_and_old_controllers_expire() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let output = temp.path().join("partial.bin");
    let id = queue.enqueue(options(&output)).unwrap();
    let directory = partial_fixture(&output, "https://other.test/file");
    assert!(queue.delete_files(id).is_err());
    assert!(directory.join("part-000").exists());
    let runner = queue.runner().unwrap();
    let controller = runner.control();
    drop(runner);
    assert!(controller.pause(id).is_err());
    assert!(!queue.jobs().unwrap()[0].is_held());
}

#[cfg(unix)]
#[test]
fn deleting_partial_data_refuses_symlinked_directories_and_parts() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let output = temp.path().join("partial.bin");
    let id = queue.enqueue(options(&output)).unwrap();
    let foreign = temp.path().join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("valuable"), b"keep").unwrap();
    symlink(&foreign, crate::state_directory(&output)).unwrap();
    assert!(queue.delete_files(id).is_err());
    assert_eq!(fs::read(foreign.join("valuable")).unwrap(), b"keep");
    fs::remove_file(crate::state_directory(&output)).unwrap();
    let directory = partial_fixture(&output, "https://example.test/archive.zip");
    fs::remove_file(directory.join("part-000")).unwrap();
    symlink(foreign.join("valuable"), directory.join("part-000")).unwrap();
    assert!(queue.delete_files(id).is_err());
    assert_eq!(fs::read(foreign.join("valuable")).unwrap(), b"keep");
}

#[test]
fn jobs_persist_with_stable_ids_and_absolute_output_paths() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("jobs");
    let queue = DownloadQueue::open(&path).unwrap();
    let relative = format!(
        "{}-relative.bin",
        temp.path().file_name().unwrap().to_string_lossy()
    );
    assert_eq!(queue.enqueue(options(&relative)).unwrap(), 1);
    drop(queue);
    let reopened = DownloadQueue::open(&path).unwrap();
    assert_eq!(
        reopened
            .enqueue(options(temp.path().join("second.bin")))
            .unwrap(),
        2
    );
    let jobs = reopened.jobs().unwrap();
    assert_eq!(jobs[0].output(), std::path::absolute(relative).unwrap());
    assert!(
        jobs.iter()
            .all(|job| job.state() == JobState::Pending && job.attempt_count() == 0)
    );
    assert!(reopened.history().unwrap().is_empty());
}

#[test]
fn concurrent_additions_do_not_lose_jobs_or_reuse_ids() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let workers: Vec<_> = (0..8)
        .map(|index| {
            let queue = queue.clone();
            let output = temp.path().join(format!("file-{index}.bin"));
            std::thread::spawn(move || queue.enqueue(options(output)).unwrap())
        })
        .collect();
    let ids: HashSet<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(ids.len(), 8);
    assert_eq!(queue.jobs().unwrap().len(), 8);
}

#[test]
fn active_runner_excludes_other_runners_but_allows_queue_reads_and_additions() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let runner = queue.runner().unwrap();
    assert!(queue.runner().is_err());
    assert!(queue.retry(1).is_err());
    queue
        .enqueue(options(temp.path().join("added-while-running.bin")))
        .unwrap();
    assert_eq!(queue.jobs().unwrap().len(), 1);
    drop(runner);
    assert!(queue.runner().is_ok());
}

#[test]
fn interrupted_running_attempt_is_recovered_without_losing_history() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let id = queue
        .enqueue(options(temp.path().join("recover.bin")))
        .unwrap();
    let runner = queue.runner().unwrap();
    assert_eq!(queue.claim_next().unwrap().unwrap().id(), id);
    drop(runner); // Simulate an execution session disappearing before finish.
    let _replacement = queue.runner().unwrap();
    assert_eq!(queue.jobs().unwrap()[0].state(), JobState::Paused);
    let history = queue.history().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].state, JobState::Paused);
    assert!(
        history[0]
            .error
            .as_deref()
            .unwrap()
            .contains("Previous runner stopped")
    );
    assert!(history[0].finished_at >= history[0].started_at);
}

#[test]
fn retry_preserves_failed_attempts_and_pending_jobs_cannot_be_retried() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let id = queue
        .enqueue(options(temp.path().join("retry.bin")))
        .unwrap();
    assert!(queue.retry(id).is_err());
    let runner = queue.runner().unwrap();
    queue.claim_next().unwrap().unwrap();
    queue
        .finish(id, JobState::Failed, None, Some("fixture failure".into()))
        .unwrap();
    drop(runner);
    queue.retry(id).unwrap();
    assert_eq!(queue.jobs().unwrap()[0].state(), JobState::Pending);
    let history = queue.history().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].error.as_deref(), Some("fixture failure"));
}

#[test]
fn invalid_or_corrupt_store_is_never_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path()).unwrap();
    let path = temp.path().join("queue.json");
    for bad in ["not json", r#"{"version":99,"next_id":1,"jobs":[]}"#] {
        fs::write(&path, bad).unwrap();
        assert!(
            queue
                .enqueue(options(temp.path().join("file.bin")))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), bad);
    }
}

#[test]
fn duplicate_outputs_and_persisted_credentials_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let output = temp.path().join("file.bin");
    assert!(
        queue
            .enqueue(options(&output).with_header("Authorization", "Bearer never-persist-this"))
            .is_err()
    );
    assert!(
        queue
            .enqueue(DownloadOptions::new(
                "https://user:password@example.test/file",
                &output
            ))
            .is_err()
    );
    queue.enqueue(options(&output)).unwrap();
    assert!(queue.enqueue(options(&output)).is_err());
    let raw = fs::read_to_string(queue.directory().join("queue.json")).unwrap();
    assert!(!raw.contains("never-persist-this") && !raw.contains("password"));
}

#[test]
fn bandwidth_limits_persist_and_old_queue_records_remain_unlimited() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    assert!(
        queue
            .enqueue(options(temp.path().join("zero.bin")).with_bandwidth_limit(0))
            .is_err()
    );
    queue
        .enqueue(options(temp.path().join("limited.bin")).with_bandwidth_limit(65536))
        .unwrap();
    queue
        .enqueue(options(temp.path().join("legacy.bin")))
        .unwrap();
    let reopened = DownloadQueue::open(queue.directory()).unwrap();
    let jobs = reopened.jobs().unwrap();
    assert_eq!(jobs[0].bandwidth_limit(), Some(65536));
    assert_eq!(jobs[0].options().bandwidth_limit(), Some(65536));
    assert_eq!(jobs[1].options().bandwidth_limit(), None);
    let path = queue.directory().join("queue.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(json["jobs"][1].get("bandwidth_limit").is_none());
    json["jobs"][0]["bandwidth_limit"] = 0.into();
    let invalid = serde_json::to_vec(&json).unwrap();
    fs::write(&path, &invalid).unwrap();
    assert!(reopened.jobs().is_err());
    assert_eq!(fs::read(path).unwrap(), invalid);
}

#[test]
fn checksums_survive_reopen_and_invalid_saved_digests_preserve_the_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let checksum: crate::Sha256Checksum = "AB".repeat(32).parse().unwrap();
    queue
        .enqueue(options(temp.path().join("verified.bin")).with_sha256(checksum.clone()))
        .unwrap();
    queue
        .enqueue(options(temp.path().join("legacy.bin")))
        .unwrap();
    let reopened = DownloadQueue::open(queue.directory()).unwrap();
    let jobs = reopened.jobs().unwrap();
    assert_eq!(jobs[0].sha256(), Some(&checksum));
    assert_eq!(jobs[0].options().sha256(), Some(&checksum));
    assert!(jobs[1].sha256().is_none());
    let path = queue.directory().join("queue.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(json["jobs"][1].get("sha256").is_none());
    json["jobs"][0]["sha256"] = "invalid".into();
    let invalid = serde_json::to_vec(&json).unwrap();
    fs::write(&path, &invalid).unwrap();
    assert!(reopened.jobs().is_err());
    assert!(
        reopened
            .enqueue(options(temp.path().join("new.bin")))
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), invalid);
}
