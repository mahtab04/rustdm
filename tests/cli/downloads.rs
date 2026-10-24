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

#[tokio::test]
async fn starting_a_selected_download_leaves_earlier_and_later_jobs_waiting() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let first = temp.path().join("first.bin");
    let selected = temp.path().join("selected.bin");
    let last = temp.path().join("last.bin");
    queue.enqueue(server.options(&first)).unwrap();
    let id = queue.enqueue(server.options(&selected)).unwrap();
    queue.enqueue(server.options(&last)).unwrap();
    queue.pause(id).unwrap();
    let mut runner = queue.runner().unwrap();
    let summary = runner
        .run_job(id, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(summary.completed, 1);
    assert_eq!(fs::read(selected).unwrap(), *server.data);
    assert!(!first.exists() && !last.exists());
    let jobs = queue.jobs().unwrap();
    assert_eq!(jobs[0].state(), JobState::Pending);
    assert_eq!(jobs[1].state(), JobState::Completed);
    assert_eq!(jobs[2].state(), JobState::Pending);
}

#[tokio::test]
async fn pausing_one_download_holds_it_while_the_queue_continues_and_selected_resume_is_exact() {
    let server = Server::new(0);
    server.slow.store(true, Ordering::SeqCst);
    let temp = tempfile::tempdir().unwrap();
    let queue = DownloadQueue::open(temp.path().join("queue")).unwrap();
    let first = temp.path().join("paused.bin");
    let second = temp.path().join("continues.bin");
    let id = queue.enqueue(server.options(&first)).unwrap();
    queue.enqueue(server.options(&second)).unwrap();
    let mut runner = queue.runner().unwrap();
    let control = runner.control();
    let paused = Arc::new(AtomicBool::new(false));
    let once = paused.clone();
    let slow = server.slow.clone();
    let summary = runner
        .run(CancellationToken::new(), move |event| match event {
            QueueEvent::Progress { job_id, progress }
                if job_id == id && progress.downloaded > 0 =>
            {
                if !once.swap(true, Ordering::SeqCst) {
                    control.pause(id).unwrap();
                }
            }
            QueueEvent::Finished { job_id, .. } if job_id == id => {
                slow.store(false, Ordering::SeqCst);
            }
            _ => {}
        })
        .await
        .unwrap();
    assert!(paused.load(Ordering::SeqCst));
    assert_eq!(summary.paused, 1);
    assert_eq!(summary.completed, 1);
    assert!(
        !summary.cancelled,
        "An individual pause must not cancel the queue"
    );
    assert!(!first.exists());
    assert_eq!(fs::read(second).unwrap(), *server.data);
    assert!(queue.jobs().unwrap()[0].is_held());
    assert_eq!(
        runner
            .run(CancellationToken::new(), |_| {})
            .await
            .unwrap()
            .completed,
        0
    );
    let resumed = runner
        .run_job(id, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(resumed.completed, 1);
    assert_eq!(fs::read(first).unwrap(), *server.data);
    assert_eq!(
        queue
            .history()
            .unwrap()
            .iter()
            .filter(|entry| entry.job_id == id)
            .count(),
        2
    );
}

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
        Self::with_size(mode_value, 131_071)
    }

    fn with_size(mode_value: u8, size: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/download", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let data = Arc::new(if mode_value == 7 {
            Vec::new()
        } else {
            (0..size)
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
    // Accepted sockets can inherit the listener's nonblocking mode on Windows.
    // Each responder has its own thread and uses blocking I/O with timeouts.
    stream.set_nonblocking(false)?;
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
    if mode == 9 || mode == 10 {
        headers.push_str("ETag: W/\"weak-fixture\"\r\n");
        if mode == 9 {
            headers.push_str("Last-Modified: Wed, 21 Oct 2015 07:28:00 GMT\r\n");
        }
    } else if mode != 6 {
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
    // Hold the first range until another connection has requested all seven
    // remaining parts. A connection tied to one fixed range cannot pass this.
    if mode == 8 && !probe && start == 0 {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.if_range.is_some())
            .count()
            < 8
        {
            if std::time::Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "Idle connection did not claim remaining parts",
                ));
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
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

// Generated independently with Python hashlib from the fixture's byte formula.
const FIXTURE_SHA256: &str = "26611906c0bdd9c797dba923fcc5dd2cb7a1c035b4260b8bd5a30bb2a6e4a670";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[tokio::test]
async fn checksums_verify_parallel_fallback_unknown_length_and_empty_files() {
    for mode in [0, 1, 5, 7] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("verified.bin");
        let checksum = if mode == 7 {
            EMPTY_SHA256
        } else {
            FIXTURE_SHA256
        };
        download(
            &server
                .options(&output)
                .with_sha256(checksum.parse().unwrap()),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(fs::read(&output).unwrap(), *server.data);
        assert!(!state_directory(&output).exists());
    }
}

#[tokio::test]
async fn checksum_mismatch_never_publishes_and_complete_resumed_parts_are_rehashed() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("mismatch.bin");
    let wrong = "0".repeat(64).parse().unwrap();
    let error = download(
        &server.options(&output).with_sha256(wrong),
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("SHA-256 mismatch"));
    assert!(error.to_string().contains(FIXTURE_SHA256));
    assert!(!output.exists());
    let sidecar = state_directory(&output);
    assert_eq!(fs::read_dir(&sidecar).unwrap().count(), 6); // manifest, lock, four parts; no temp assembly
    // Corrupt a saved byte without changing part size: resume must verify it too.
    let part = sidecar.join("part-000");
    let mut bytes = fs::read(&part).unwrap();
    bytes[0] ^= 0xff;
    fs::write(&part, &bytes).unwrap();
    let options = server
        .options(&output)
        .with_resume(true)
        .with_sha256(FIXTURE_SHA256.parse().unwrap());
    assert!(
        download(&options, CancellationToken::new(), |_| {})
            .await
            .unwrap_err()
            .to_string()
            .contains("SHA-256 mismatch")
    );
    assert!(!output.exists());
    bytes[0] ^= 0xff;
    fs::write(&part, bytes).unwrap();
    let summary = download(&options, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(summary.resumed_bytes, server.data.len() as u64);
    assert_eq!(fs::read(&output).unwrap(), *server.data);
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.if_range.is_some())
            .count(),
        4
    );
}

#[test]
fn cli_queue_verifies_persisted_checksums_and_records_mismatch_history() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("jobs");
    let bad = temp.path().join("bad.bin");
    let good = temp.path().join("good.bin");
    let invoke = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_rustdm"))
            .arg("--state-dir")
            .arg(&state)
            .args(args)
            .output()
            .unwrap()
    };
    for (path, checksum) in [
        (&bad, "0".repeat(64)),
        (&good, FIXTURE_SHA256.to_uppercase()),
    ] {
        let result = invoke(&[
            "queue",
            "add",
            &server.url,
            "-o",
            path.to_str().unwrap(),
            "--sha256",
            &checksum,
        ]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let result = invoke(&["queue", "run"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("SHA-256 mismatch"));
    assert!(!bad.exists());
    assert_eq!(fs::read(&good).unwrap(), *server.data);
    let queue = DownloadQueue::open(&state).unwrap();
    assert_eq!(
        queue.jobs().unwrap()[1].sha256().unwrap().to_string(),
        FIXTURE_SHA256
    );
    let history = queue.history().unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].state, JobState::Failed);
    assert!(
        history[0]
            .error
            .as_deref()
            .unwrap()
            .contains("SHA-256 mismatch")
    );
    assert_eq!(history[1].state, JobState::Completed);
    assert!(invoke(&["queue", "retry", "1"]).status.success());
    assert!(!invoke(&["queue", "run"]).status.success());
    assert!(!bad.exists());
    assert_eq!(queue.history().unwrap().len(), 3);
}

#[test]
fn cli_direct_checksum_mismatch_exits_nonzero_and_accepts_a_corrected_digest_on_resume() {
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("cli-verified.bin");
    let invoke = |checksum: &str, resume: bool| {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_rustdm"));
        command
            .args(["download", &server.url, "-o"])
            .arg(&output)
            .args(["--sha256", checksum]);
        if resume {
            command.arg("--resume");
        }
        command.output().unwrap()
    };
    let bad = invoke(&"0".repeat(64), false);
    assert!(!bad.status.success());
    assert!(!output.exists());
    let good = invoke(FIXTURE_SHA256, true);
    assert!(
        good.status.success(),
        "{}",
        String::from_utf8_lossy(&good.stderr)
    );
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn free_connections_claim_more_ranges_while_the_first_range_is_blocked() {
    let server = Server::with_size(8, 2 * 1024 * 1024);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("scheduled.bin");
    let summary = tokio::time::timeout(
        Duration::from_secs(10),
        download(
            &server
                .options(&output)
                .with_connections(2)
                .with_retries(0)
                .with_sha256(
                    "44edc80377f7d227f3e05a12c7a6458262ffa104db638068e7f48cae2917a26e"
                        .parse()
                        .unwrap(),
                ),
            CancellationToken::new(),
            |_| {},
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(summary.connections, 2);
    assert_eq!(fs::read(output).unwrap(), *server.data);
    let requests = server.requests.lock().unwrap();
    let mut ranges: Vec<_> = requests
        .iter()
        .filter(|r| r.if_range.is_some())
        .map(|r| r.range.unwrap())
        .collect();
    ranges.sort_unstable();
    assert_eq!(ranges.len(), 8);
    let mut next = 0;
    for (start, end) in ranges {
        assert_eq!(start, next);
        next = end + 1;
    }
    assert_eq!(next, server.data.len() as u64);
}

#[tokio::test]
async fn dynamically_scheduled_parts_resume_with_a_lower_connection_limit() {
    let server = Server::with_size(0, 8 * 1024 * 1024);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("scheduled-resume.bin");
    pause(&server, &server.options(&output).with_connections(4)).await;
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(state_directory(&output).join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["segments"].as_array().unwrap().len(), 32);
    let summary = download(
        &server
            .options(&output)
            .with_connections(1)
            .with_resume(true)
            .with_sha256(
                "a6a7387ed918a4fe3e11b7ffa8055ba6a1dfccb86e4f64df283ed832b9fe0ad2"
                    .parse()
                    .unwrap(),
            ),
        CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(summary.connections, 1);
    assert!(summary.resumed_bytes > 0);
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn weak_etags_use_last_modified_for_ranges_or_fall_back_without_it() {
    for mode in [9, 10] {
        let server = Server::new(mode);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("weak-etag.bin");
        let summary = download(
            &server
                .options(&output)
                .with_sha256(FIXTURE_SHA256.parse().unwrap()),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(summary.connections, if mode == 9 { 4 } else { 1 });
        assert_eq!(summary.resumable, mode == 9);
        assert_eq!(fs::read(output).unwrap(), *server.data);
        let requests = server.requests.lock().unwrap();
        for request in requests.iter().filter(|request| request.if_range.is_some()) {
            assert_eq!(
                request.if_range.as_deref(),
                Some("Wed, 21 Oct 2015 07:28:00 GMT")
            );
        }
    }
}

#[derive(Clone)]
struct ConnectionLog {
    events: Arc<Mutex<Vec<rustdm_core::ConnectionProgress>>>,
    cancel: Option<CancellationToken>,
}
impl ProgressObserver for ConnectionLog {
    fn on_progress(&self, _: Progress) {}
    fn on_connection(&self, event: rustdm_core::ConnectionProgress) {
        self.events.lock().unwrap().push(event);
        if event.downloaded >= 2048
            && let Some(cancel) = &self.cancel
        {
            cancel.cancel();
        }
    }
}

#[tokio::test]
async fn connection_telemetry_tracks_workers_across_dynamic_ranges_and_retries() {
    use rustdm_core::ConnectionStatus;
    for mode in [0, 1, 4, 5] {
        let server = Server::with_size(mode, 2 * 1024 * 1024);
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("connections.bin");
        let events = Arc::new(Mutex::new(Vec::new()));
        let observer = ConnectionLog {
            events: events.clone(),
            cancel: None,
        };
        let result = DownloadManager::new(server.options(&output).with_connections(2))
            .download(CancellationToken::new(), observer)
            .await
            .unwrap();
        let events = events.lock().unwrap();
        let mut last = std::collections::BTreeMap::new();
        for event in events.iter() {
            assert!((1..=result.connections).contains(&event.id));
            if let Some(previous) = last.insert(event.id, *event) {
                assert!(event.downloaded >= previous.downloaded);
            }
        }
        assert_eq!(last.len(), result.connections);
        assert!(
            last.values()
                .all(|event| event.status == ConnectionStatus::Completed)
        );
        assert_eq!(
            last.values().map(|event| event.downloaded).sum::<u64>(),
            server.data.len() as u64
        );
        assert!(
            events
                .iter()
                .any(|event| event.status == ConnectionStatus::Sending)
        );
        assert!(
            events
                .iter()
                .any(|event| event.status == ConnectionStatus::Receiving)
        );
        if mode == 4 {
            assert!(
                events
                    .iter()
                    .any(|event| event.status == ConnectionStatus::Retrying)
            );
        }
        assert_eq!(fs::read(output).unwrap(), *server.data);
    }
}

#[tokio::test]
async fn connection_telemetry_records_pause_and_excludes_saved_resume_bytes() {
    use rustdm_core::ConnectionStatus;
    let server = Server::new(0);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("connection-resume.bin");
    let cancel = CancellationToken::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let observer = ConnectionLog {
        events: events.clone(),
        cancel: Some(cancel.clone()),
    };
    assert!(
        DownloadManager::new(server.options(&output))
            .download(cancel, observer)
            .await
            .is_err()
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.status == ConnectionStatus::Paused)
    );
    let resumed_events = Arc::new(Mutex::new(Vec::new()));
    let result = DownloadManager::new(server.options(&output).with_resume(true))
        .download(
            CancellationToken::new(),
            ConnectionLog {
                events: resumed_events.clone(),
                cancel: None,
            },
        )
        .await
        .unwrap();
    assert!(result.resumed_bytes > 0);
    let mut last = std::collections::BTreeMap::new();
    for event in resumed_events.lock().unwrap().iter() {
        last.insert(event.id, *event);
    }
    assert_eq!(
        last.values().map(|event| event.downloaded).sum::<u64>(),
        result.bytes - result.resumed_bytes
    );
    assert_eq!(fs::read(output).unwrap(), *server.data);
}

#[tokio::test]
async fn invalid_range_reports_a_failed_connection_without_publishing() {
    let server = Server::new(2);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("connection-fail.bin");
    let events = Arc::new(Mutex::new(Vec::new()));
    assert!(
        DownloadManager::new(server.options(&output).with_retries(0))
            .download(
                CancellationToken::new(),
                ConnectionLog {
                    events: events.clone(),
                    cancel: None
                }
            )
            .await
            .is_err()
    );
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.status == rustdm_core::ConnectionStatus::Failed)
    );
    assert!(!output.exists());
}
