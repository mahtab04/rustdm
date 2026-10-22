use super::{DownloadQueue, JobState};
use crate::DownloadOptions;
use std::{collections::HashSet, fs};

fn options(output: impl Into<std::path::PathBuf>) -> DownloadOptions {
    DownloadOptions::new("https://example.test/archive.zip", output)
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
