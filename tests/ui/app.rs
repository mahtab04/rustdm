use super::*;
use rustdm_core::DownloadQueue;

#[test]
fn closing_during_enqueue_saves_the_job_without_starting_a_transfer() {
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("state");
    let output = temporary.path().join("untouched.bin");
    let mut app = DesktopApp::from_context(&egui::Context::default(), state.clone()).unwrap();
    app.adding = true;
    app.backend.add(
        rustdm_core::DownloadOptions::new("http://127.0.0.1:9/file", &output),
        true,
    );
    app.begin_close();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.adding && Instant::now() < deadline {
        app.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.adding, "Pending addition must finish before closing");
    assert!(app.closing);
    assert!(
        !app.backend.running(),
        "Download now must not restart work during shutdown"
    );
    let queue = DownloadQueue::open(state).unwrap();
    let jobs = queue.jobs().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].state(), JobState::Pending);
    assert!(queue.history().unwrap().is_empty());
    assert!(!output.exists());
}

#[test]
fn corrupt_preferences_warn_without_preventing_queue_access_or_rewriting_the_file() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("preferences.json");
    std::fs::write(&path, "malformed preferences").unwrap();
    let app =
        DesktopApp::from_context(&egui::Context::default(), temporary.path().to_owned()).unwrap();
    assert!(
        app.notice
            .as_ref()
            .unwrap()
            .contains("Settings unavailable")
    );
    assert_eq!(app.form.connections, 4);
    assert!(
        DownloadQueue::open(temporary.path())
            .unwrap()
            .jobs()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "malformed preferences"
    );
}
