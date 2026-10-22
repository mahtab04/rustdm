use rustdm_core::{
    CancellationToken, DownloadManager, DownloadOptions, DownloadQueue, JobState, Progress,
    ProgressObserver, QueueEvent, download, state_directory,
};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug)]
struct Request {
    range: Option<(u64, u64)>,
    if_range: Option<String>,
    raw: String,
}

// A real local HTTP peer: modes deliberately violate or omit range support.
// No internet service or copied implementation is used as the test oracle.
struct Server {
    url: String,
    data: Arc<Vec<u8>>,
    slow: Arc<AtomicBool>,
    etag: Arc<Mutex<String>>,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn new(mode_value: u8) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/download", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let data = Arc::new(if mode_value == 7 {
            Vec::new()
        } else {
            (0..131_071)
                .map(|i| ((i * 31) % 251) as u8)
                .collect::<Vec<_>>()
        });
        let mode = Arc::new(AtomicU8::new(mode_value));
        let slow = Arc::new(AtomicBool::new(false));
        let etag = Arc::new(Mutex::new("\"fixture-v1\"".to_string()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (body, state, delay, tag, log, done) = (
            data.clone(),
            mode.clone(),
            slow.clone(),
            etag.clone(),
            requests.clone(),
            stop.clone(),
        );
        let handle = thread::spawn(move || {
            let mut workers = Vec::new();
            let truncated = Arc::new(AtomicBool::new(false));
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (body, state, delay, tag, log, truncated) = (
                            body.clone(),
                            state.clone(),
                            delay.clone(),
                            tag.clone(),
                            log.clone(),
                            truncated.clone(),
                        );
                        workers.push(thread::spawn(move || {
                            let _ = respond(stream, &body, &state, &delay, &tag, &log, &truncated);
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("Fixture accept failed: {error}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url,
            data,
            slow,
            etag,
            requests,
            stop,
            thread: Some(handle),
        }
    }

    fn options(&self, output: impl Into<std::path::PathBuf>) -> DownloadOptions {
        DownloadOptions::new(&self.url, output)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn respond(
    mut stream: TcpStream,
    data: &[u8],
    mode: &AtomicU8,
    slow: &AtomicBool,
    etag: &Mutex<String>,
    requests: &Mutex<Vec<Request>>,
    truncated: &AtomicBool,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = Vec::new();
    let mut byte = [0];
    while !buffer.ends_with(b"\r\n\r\n") && buffer.len() < 65536 {
        if stream.read(&mut byte)? == 0 {
            return Ok(());
        }
        buffer.push(byte[0]);
    }
    let raw = String::from_utf8(buffer).unwrap();
    let header = |name: &str| {
        raw.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let range = header("range").map(|value| {
        let (start, end) = value
            .strip_prefix("bytes=")
            .unwrap()
            .split_once('-')
            .unwrap();
        (start.parse::<u64>().unwrap(), end.parse::<u64>().unwrap())
    });
    let if_range = header("if-range");
    requests.lock().unwrap().push(Request {
        range,
        if_range,
        raw,
    });
    let mode = mode.load(Ordering::SeqCst);
    if data.is_empty() && range.is_some() {
        stream.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
        return Ok(());
    }
    let probe = range == Some((0, 0));
    let partial = range.is_some() && mode != 1 && mode != 5 && !(mode == 3 && !probe);
    let (start, end) = if partial {
        range.unwrap()
    } else {
        (0, data.len().saturating_sub(1) as u64)
    };
    let body = if data.is_empty() {
        &[][..]
    } else {
        &data[start as usize..=end as usize]
    };
    let status = if partial {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let mut headers = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
    if mode != 6 {
        headers.push_str(&format!("ETag: {}\r\n", etag.lock().unwrap()));
    }
    if mode != 5 {
        headers.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    if partial {
        let reported_start = if mode == 2 && !probe {
            start + 1
        } else {
            start
        };
        headers.push_str(&format!(
            "Content-Range: bytes {reported_start}-{end}/{}\r\n",
            data.len()
        ));
    }
    headers.push_str("\r\n");
    stream.write_all(headers.as_bytes())?;
    let body = if mode == 4 && !probe && !truncated.swap(true, Ordering::SeqCst) {
        &body[..body.len() / 2]
    } else {
        body
    };
    for chunk in body.chunks(1024) {
        stream.write_all(chunk)?;
        if !probe && slow.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}

#[tokio::test]
async fn parallel_download_is_byte_exact_and_forwards_request_context() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("parallel.bin");
    let mut options = server.options(&output);
    options = options.with_header("Referer", "https://example.test/page");
    let result = download(&options, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(fs::read(&output).unwrap(), *server.data);
    assert_eq!(result.connections, 4);
    assert_eq!(result.bytes, server.data.len() as u64);
    assert!(!state_directory(&output).exists());
    let requests = server.requests.lock().unwrap();
    let parts: Vec<_> = requests.iter().filter(|r| r.if_range.is_some()).collect();
    assert_eq!(parts.len(), 4);
    assert!(
        parts
            .iter()
            .all(|r| r.if_range.as_deref() == Some("\"fixture-v1\""))
    );
    assert!(requests.iter().all(|r| {
        r.raw
            .to_lowercase()
            .contains("referer: https://example.test/page")
    }));
}

#[tokio::test]
async fn ignored_ranges_fall_back_to_a_single_correct_stream() {
    let server = Server::new(1);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("single.bin");
    let result = download(&server.options(&output), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(result.connections, 1);
    assert!(!result.resumable);
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn unknown_length_stream_and_empty_file_are_supported() {
    for mode in [5, 7] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("stream.bin");
        let result = download(&server.options(&output), CancellationToken::new(), |_| {})
            .await
            .unwrap();
        assert_eq!(result.bytes, server.data.len() as u64);
        assert_eq!(fs::read(output).unwrap(), *server.data);
    }
}

#[tokio::test]
async fn missing_validator_disables_segmenting_and_unsafe_resume() {
    let server = Server::new(6);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("no-validator.bin");
    let result = download(&server.options(&output), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(result.connections, 1);
    assert!(!result.resumable);
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn malformed_ranges_and_full_responses_never_publish_a_file() {
    for mode in [2, 3] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("invalid.bin");
        assert!(
            download(
                &server.options(&output).with_retries(0),
                CancellationToken::new(),
                |_| {}
            )
            .await
            .is_err()
        );
        assert!(!output.exists());
        assert!(state_directory(&output).join("manifest.json").exists());
    }
}

#[tokio::test]
async fn interrupted_connection_retries_only_missing_bytes() {
    let server = Server::new(4);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("retry.bin");
    let mut options = server.options(&output);
    options = options.with_connections(1);
    options = options.with_retries(1);
    download(&options, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(fs::read(output).unwrap(), *server.data);
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| { r.if_range.is_some() && r.range.is_some_and(|(start, _)| start > 0) })
    );
}

async fn pause(server: &Server, options: &DownloadOptions) {
    server.slow.store(true, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let result = download(options, cancel, move |progress| {
        if progress.downloaded >= 2048 {
            trigger.cancel();
        }
    })
    .await;
    assert!(result.is_err());
    assert!(!options.output().exists());
    server.slow.store(false, Ordering::SeqCst);
}

#[tokio::test]
async fn pause_and_resume_preserve_bytes_across_invocations() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("resume.bin");
    let mut options = server.options(&output);
    pause(&server, &options).await;
    assert!(
        download(&options, CancellationToken::new(), |_| {})
            .await
            .is_err()
    );
    options = options.with_resume(true);
    let result = download(&options, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert!(result.resumed_bytes > 0);
    assert!(result.resumed_bytes < result.bytes);
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn changed_remote_file_is_rejected_before_reusing_saved_parts() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("changed.bin");
    let mut options = server.options(&output);
    pause(&server, &options).await;
    *server.etag.lock().unwrap() = "\"fixture-v2\"".into();
    options = options.with_resume(true);
    let error = download(&options, CancellationToken::new(), |_| {})
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Remote file changed"));
    assert!(!output.exists());
}

#[tokio::test]
async fn existing_output_and_unknown_partial_files_are_preserved() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("existing.bin");
    fs::write(&output, b"keep me").unwrap();
    assert!(
        download(&server.options(&output), CancellationToken::new(), |_| {})
            .await
            .is_err()
    );
    assert_eq!(fs::read(&output).unwrap(), b"keep me");
    let other = temp.path().join("other.bin");
    let state = state_directory(&other);
    fs::create_dir(&state).unwrap();
    fs::write(state.join("unrelated"), b"keep this too").unwrap();
    assert!(
        download(&server.options(&other), CancellationToken::new(), |_| {})
            .await
            .is_err()
    );
    assert_eq!(fs::read(state.join("unrelated")).unwrap(), b"keep this too");
    assert!(!other.exists());
}

#[test]
fn cli_downloads_a_file_with_the_real_executable() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("cli.bin");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_rustdm"))
        .arg(&server.url)
        .args(["--connections", "3", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(output).unwrap(), *server.data);
    assert!(String::from_utf8_lossy(&result.stdout).contains("3 connections"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("ETA"));
}

#[tokio::test]
async fn destination_created_during_download_is_never_overwritten() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("racing.bin");
    let other_writer = output.clone();
    let wrote = AtomicBool::new(false);
    let result = download(
        &server.options(&output),
        CancellationToken::new(),
        move |progress| {
            if progress.downloaded > 0 && !wrote.swap(true, Ordering::SeqCst) {
                fs::write(&other_writer, b"written by somebody else").unwrap();
            }
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(fs::read(output).unwrap(), b"written by somebody else");
}

struct RecordingObserver(Arc<AtomicU64>);

impl ProgressObserver for RecordingObserver {
    fn on_progress(&self, progress: Progress) {
        self.0.fetch_max(progress.downloaded, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn manager_accepts_a_custom_progress_observer() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("observer.bin");
    let observed = Arc::new(AtomicU64::new(0));
    let manager = DownloadManager::new(server.options(&output));
    assert_eq!(manager.options().output(), output);
    let summary = manager
        .download(
            CancellationToken::new(),
            RecordingObserver(observed.clone()),
        )
        .await
        .unwrap();
    assert_eq!(observed.load(Ordering::SeqCst), summary.bytes);
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn renamed_app_resumes_legacy_rustidm_partial_files() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("legacy.bin");
    let options = server.options(&output);
    pause(&server, &options).await;
    let current = state_directory(&output);
    let mut legacy = output.as_os_str().to_os_string();
    legacy.push(".rustidm");
    let legacy = std::path::PathBuf::from(legacy);
    assert!(current.starts_with(temp.path()) && legacy.starts_with(temp.path()));
    fs::rename(&current, &legacy).unwrap();
    assert_eq!(state_directory(&output), legacy);
    let result = download(&options.with_resume(true), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert!(result.resumed_bytes > 0);
    assert_eq!(fs::read(output).unwrap(), *server.data);
    assert!(!legacy.exists());
}

#[tokio::test]
async fn queued_pause_resumes_in_order_and_keeps_attempt_history() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("jobs");
    let first = temp.path().join("first.bin");
    let second = temp.path().join("second.bin");
    let queue = DownloadQueue::open(&directory).unwrap();
    let id = queue.enqueue(server.options(&first)).unwrap();
    queue.enqueue(server.options(&second)).unwrap();
    server.slow.store(true, Ordering::SeqCst);
    let token = CancellationToken::new();
    let trigger = token.clone();
    let mut runner = queue.runner().unwrap();
    let result = runner
        .run(token, move |event| {
            if let QueueEvent::Progress { job_id, progress } = event
                && job_id == id
                && progress.downloaded >= 2048
            {
                trigger.cancel();
            }
        })
        .await
        .unwrap();
    assert!(result.cancelled);
    assert_eq!(result.paused, 1);
    let jobs = queue.jobs().unwrap();
    assert_eq!(jobs[0].state(), JobState::Paused);
    assert_eq!(jobs[1].state(), JobState::Pending);
    assert!(!first.exists() && !second.exists());
    drop(runner);
    server.slow.store(false, Ordering::SeqCst);
    let reopened = DownloadQueue::open(directory).unwrap();
    let result = reopened
        .runner()
        .unwrap()
        .run(CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(result.completed, 2);
    assert_eq!(fs::read(first).unwrap(), *server.data);
    assert_eq!(fs::read(second).unwrap(), *server.data);
    let history = reopened.history().unwrap();
    assert_eq!(history.len(), 3);
    assert!(history.iter().any(|entry| entry.job_id == id
        && entry.state == JobState::Completed
        && entry.resumed_bytes.unwrap() > 0));
}

#[tokio::test]
async fn failed_queue_job_does_not_block_later_jobs_and_retry_keeps_history() {
    let interrupted = Server::new(4); // Exactly one truncated response, then normal responses.
    let healthy = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("retry-first.bin");
    let second = temp.path().join("healthy-second.bin");
    let queue = DownloadQueue::open(temp.path().join("jobs")).unwrap();
    let id = queue
        .enqueue(
            interrupted
                .options(&first)
                .with_connections(1)
                .with_retries(0),
        )
        .unwrap();
    queue.enqueue(healthy.options(&second)).unwrap();
    let result = queue
        .runner()
        .unwrap()
        .run(CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(result.failed, 1);
    assert_eq!(result.completed, 1);
    assert!(!first.exists());
    assert_eq!(fs::read(second).unwrap(), *healthy.data);
    queue.retry(id).unwrap();
    let result = queue
        .runner()
        .unwrap()
        .run(CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(result.completed, 1);
    assert_eq!(fs::read(first).unwrap(), *interrupted.data);
    let attempts: Vec<_> = queue
        .history()
        .unwrap()
        .into_iter()
        .filter(|entry| entry.job_id == id)
        .collect();
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].state, JobState::Failed);
    assert_eq!(attempts[1].state, JobState::Completed);
    assert!(attempts[1].resumed_bytes.unwrap() > 0);
}

#[test]
fn cli_queue_and_history_survive_separate_processes() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("state");
    let first = temp.path().join("cli-queue-first.bin");
    let second = temp.path().join("cli-queue-second.bin");
    let command = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_rustdm"))
            .arg("--state-dir")
            .arg(&state)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    for output in [&first, &second] {
        command(&["queue", "add", &server.url, "-o", output.to_str().unwrap()]);
    }
    let jobs = command(&["queue", "list", "--json"]);
    let jobs: serde_json::Value = serde_json::from_slice(&jobs.stdout).unwrap();
    assert_eq!(jobs.as_array().unwrap().len(), 2);
    assert_eq!(jobs[0]["state"], "pending");
    command(&["queue", "run"]);
    let history = command(&["history", "--json"]);
    let history: serde_json::Value = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_eq!(history[0]["state"], "completed");
    assert_eq!(fs::read(first).unwrap(), *server.data);
    assert_eq!(fs::read(second).unwrap(), *server.data);
    assert!(command(&["queue", "run"]).status.success());
    let unchanged_history = command(&["history", "--json"]);
    let unchanged_history: serde_json::Value =
        serde_json::from_slice(&unchanged_history.stdout).unwrap();
    assert_eq!(unchanged_history.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn bandwidth_limit_is_aggregate_for_parallel_fallback_and_unknown_length() {
    // Four independent per-connection caps would incorrectly finish in ~0.5s.
    for mode in [0, 1, 4, 5] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("limited.bin");
        let start = std::time::Instant::now();
        let summary = tokio::time::timeout(
            Duration::from_secs(20),
            download(
                &server.options(&output).with_bandwidth_limit(65536),
                CancellationToken::new(),
                |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            start.elapsed() >= Duration::from_millis(1950),
            "Aggregate cap exceeded in mode {mode}"
        );
        assert_eq!(
            summary.connections,
            if mode == 0 || mode == 4 { 4 } else { 1 }
        );
        assert_eq!(fs::read(output).unwrap(), *server.data);
    }
}

#[tokio::test]
async fn throttled_waits_cancel_promptly_and_resume_with_a_new_rate() {
    for mode in [0, 1] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("paused-limit.bin");
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let (result, ()) = tokio::join!(
            async {
                tokio::time::timeout(
                    Duration::from_secs(3),
                    download(
                        &server.options(&output).with_bandwidth_limit(1),
                        cancel,
                        |_| {},
                    ),
                )
                .await
                .unwrap()
            },
            async {
                tokio::time::sleep(Duration::from_millis(150)).await;
                trigger.cancel();
            }
        );
        assert!(result.is_err());
        assert!(!output.exists());
        if mode == 0 {
            download(
                &server
                    .options(&output)
                    .with_resume(true)
                    .with_bandwidth_limit(262144),
                CancellationToken::new(),
                |_| {},
            )
            .await
            .unwrap();
            assert_eq!(fs::read(output).unwrap(), *server.data);
        }
    }
}

#[tokio::test]
async fn zero_bandwidth_is_rejected_before_creating_download_state() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("zero.bin");
    let error = download(
        &DownloadOptions::new("http://127.0.0.1:1/file", &output).with_bandwidth_limit(0),
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("greater than zero"));
    assert!(!state_directory(&output).exists());
}

#[test]
fn cli_persists_and_executes_a_queued_bandwidth_limit() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("jobs");
    let output = temp.path().join("queued-limit.bin");
    let invoke = |args: &[&str]| {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_rustdm"))
            .arg("--state-dir")
            .arg(&state)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    };
    invoke(&[
        "queue",
        "add",
        &server.url,
        "-o",
        output.to_str().unwrap(),
        "--limit-rate",
        "65536",
    ]);
    let queue = DownloadQueue::open(&state).unwrap();
    assert_eq!(queue.jobs().unwrap()[0].bandwidth_limit(), Some(65536));
    let start = std::time::Instant::now();
    invoke(&["queue", "run"]);
    assert!(start.elapsed() >= Duration::from_millis(1950));
    assert_eq!(fs::read(output).unwrap(), *server.data);
    assert_eq!(queue.jobs().unwrap()[0].state(), JobState::Completed);
}
