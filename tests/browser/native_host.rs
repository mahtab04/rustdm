use rustdm_core::{DownloadOptions, DownloadQueue, JobState};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

const CALLER: &str = "chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/";

struct Fixture {
    directory: tempfile::TempDir,
    config: PathBuf,
    state: PathBuf,
    downloads: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("browser-host.json");
        let state = directory.path().join("state");
        let downloads = directory.path().join("downloads");
        fs::create_dir(&downloads).unwrap();
        fs::write(
            &config,
            serde_json::to_vec(&json!({
                "version":1, "state_dir":state, "download_directory":downloads,
                "allowed_callers":[CALLER, "rustdm@mahtab04"]
            }))
            .unwrap(),
        )
        .unwrap();
        Self {
            directory,
            config,
            state,
            downloads,
        }
    }

    fn run(&self, bytes: &[u8], arguments: &[&str]) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rustdm-browser-host"))
            .arg("--config")
            .arg(&self.config)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        child.wait_with_output().unwrap()
    }

    fn messages(&self, requests: &[Value]) -> Vec<Value> {
        let bytes: Vec<u8> = requests.iter().flat_map(frame).collect();
        let output = self.run(&bytes, &[CALLER]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        decode(&output.stdout)
    }
}

fn request(filename: &str) -> Value {
    json!({"version":1,"id":"test-request","action":"enqueue","url":"https://example.test/file","filename":filename})
}

fn frame(value: &Value) -> Vec<u8> {
    let payload = serde_json::to_vec(value).unwrap();
    let mut bytes = (payload.len() as u32).to_ne_bytes().to_vec();
    bytes.extend(payload);
    bytes
}

fn decode(mut bytes: &[u8]) -> Vec<Value> {
    let mut values = Vec::new();
    while !bytes.is_empty() {
        let length = u32::from_ne_bytes(bytes[..4].try_into().unwrap()) as usize;
        values.push(serde_json::from_slice(&bytes[4..4 + length]).unwrap());
        bytes = &bytes[4 + length..];
    }
    values
}

#[test]
fn framed_requests_queue_unicode_filenames_without_starting_downloads() {
    let fixture = Fixture::new();
    let replies = fixture.messages(&[request("résumé.txt"), request("second.bin")]);
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0]["ok"], true);
    assert_eq!(replies[1]["job_id"], 2);
    let queue = DownloadQueue::open(&fixture.state).unwrap();
    let jobs = queue.jobs().unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|job| job.state() == JobState::Pending));
    assert!(queue.history().unwrap().is_empty());
    assert_eq!(jobs[0].output(), fixture.downloads.join("résumé.txt"));
    assert_eq!(fs::read_dir(&fixture.downloads).unwrap().count(), 0);
}

#[test]
fn filename_rules_match_the_shared_extension_corpus() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("filename_cases.json")).unwrap();
    for case in cases {
        let fixture = Fixture::new();
        let name = case["name"].as_str().unwrap();
        let replies = fixture.messages(&[request(name)]);
        assert_eq!(replies[0]["ok"], case["valid"], "Filename: {name:?}");
        if case["valid"] == false {
            assert!(!fixture.state.join("queue.json").exists());
        }
    }
}

#[test]
fn invalid_urls_options_versions_and_actions_do_not_enqueue() {
    let fixture = Fixture::new();
    let mut requests = Vec::new();
    for url in [
        "file:///secret",
        "javascript:alert(1)",
        "https://user:password@example.test/file",
        "ftp://example.test/file",
    ] {
        let mut value = request("bad.bin");
        value["url"] = json!(url);
        requests.push(value);
    }
    let mut value = request("bad.bin");
    value["headers"] = json!({"Cookie":"secret"});
    requests.push(value);
    let mut value = request("bad.bin");
    value["version"] = json!(2);
    requests.push(value);
    let mut value = request("bad.bin");
    value["action"] = json!("run_command");
    requests.push(value);
    let mut value = request("bad.bin");
    value["filename"] = json!("x".repeat(181));
    requests.push(value);
    let replies = fixture.messages(&requests);
    assert!(replies.iter().all(|reply| reply["ok"] == false));
    assert!(
        DownloadQueue::open(&fixture.state)
            .unwrap()
            .jobs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn collisions_and_existing_files_are_preserved() {
    let fixture = Fixture::new();
    let queue = DownloadQueue::open(&fixture.state).unwrap();
    queue
        .enqueue(DownloadOptions::new(
            "https://example.test/desktop",
            fixture.downloads.join("desktop.bin"),
        ))
        .unwrap();
    fs::write(fixture.downloads.join("existing.bin"), b"preserve").unwrap();
    let replies = fixture.messages(&[
        request("same.bin"),
        request("same.bin"),
        request("existing.bin"),
        request("desktop.bin"),
    ]);
    assert_eq!(replies[0]["ok"], true);
    assert_eq!(replies[1]["ok"], false);
    assert_eq!(replies[2]["ok"], false);
    assert_eq!(
        replies[3]["ok"], false,
        "A desktop/CLI job must also prevent a duplicate browser destination"
    );
    assert_eq!(
        fs::read(fixture.downloads.join("existing.bin")).unwrap(),
        b"preserve"
    );
    assert_eq!(
        DownloadQueue::open(&fixture.state)
            .unwrap()
            .jobs()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn unknown_callers_cannot_create_queue_state_and_firefox_can_enqueue() {
    let fixture = Fixture::new();
    for args in [
        vec!["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"],
        vec!["--parent-window=123"],
        vec![CALLER, "--parent-window=oops"],
    ] {
        let output = fixture.run(&[], &args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!fixture.state.exists());
    }
    let path = fixture.directory.path().join("com.rustdm.bridge.json");
    let output = fixture.run(
        &frame(&request("firefox.bin")),
        &[path.to_str().unwrap(), "rustdm@mahtab04"],
    );
    assert!(output.status.success());
    assert_eq!(decode(&output.stdout)[0]["ok"], true);
}

#[test]
fn oversized_zero_and_truncated_frames_fail_without_protocol_noise() {
    let fixture = Fixture::new();
    for bytes in [
        65537_u32.to_ne_bytes().to_vec(),
        vec![0; 4],
        vec![1, 0],
        vec![2, 0, 0, 0, b'{'],
    ] {
        let output = fixture.run(&bytes, &[CALLER]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!fixture.state.exists());
    }
    let output = fixture.run(&[], &[CALLER]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn malformed_json_gets_a_framed_error_and_later_requests_still_work() {
    let fixture = Fixture::new();
    let mut bytes = vec![1, 0, 0, 0, b'{'];
    bytes.extend(frame(&request("valid.bin")));
    let output = fixture.run(&bytes, &[CALLER, "--parent-window=0"]);
    assert!(output.status.success());
    let replies = decode(&output.stdout);
    assert_eq!(replies[0]["ok"], false);
    assert_eq!(replies[1]["ok"], true);
}
