use super::*;
use rustdm_core::DownloadQueue;

#[test]
fn queue_can_start_pause_and_quit_using_only_hidden_logic() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = 0;
        while requests < 2 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 262144\r\nConnection: close\r\n\r\n",
            );
            let _ = stream.write_all(&vec![42; 262144]);
            requests += 1;
        }
        assert_eq!(requests, 2);
    });
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("state");
    let output = temporary.path().join("paused.bin");
    let queue = DownloadQueue::open(&state).unwrap();
    let id = queue
        .enqueue(
            rustdm_core::DownloadOptions::new(url, &output)
                .with_connections(1)
                .with_bandwidth_limit(8192),
        )
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = DesktopApp::from_context(&ctx, state).unwrap();
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_tray_action(Action::Start, ctx)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.background_logic(ctx));
        if app
            .metrics
            .get(&id)
            .is_some_and(|m| m.progress.downloaded > 0)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        app.metrics
            .get(&id)
            .is_some_and(|m| m.progress.downloaded > 0)
    );
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_tray_action(Action::Pause, ctx)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.backend.running() && Instant::now() < deadline {
        let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| app.background_logic(ctx));
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.backend.running());
    assert_eq!(queue.jobs().unwrap()[0].state(), JobState::Paused);
    assert_eq!(queue.history().unwrap()[0].state, JobState::Paused);
    assert!(!output.exists());
    let result = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_tray_action(Action::Quit, ctx);
        app.background_logic(ctx);
    });
    assert!(
        result.viewport_commands[&egui::ViewportId::ROOT]
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Close))
    );
    server.join().unwrap();
}

#[test]
fn hidden_logic_restores_the_window_and_quit_waits_for_pending_work() {
    let temporary = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut app = DesktopApp::from_context(&ctx, temporary.path().to_owned()).unwrap();
    app.preferences.close_to_tray = true;
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_tray_action(Action::Show, ctx);
    });
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(true)))
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Minimized(false)))
    );
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Focus))
    );

    app.picking = true;
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
        app.handle_tray_action(Action::Quit, ctx);
        app.handle_tray_action(Action::Start, ctx);
        app.background_logic(ctx);
    });
    assert!(app.closing, "Quit must override close-to-tray preferences");
    assert!(
        !app.backend.running(),
        "Start cannot restart work during shutdown"
    );
    assert!(
        !output.viewport_commands[&egui::ViewportId::ROOT]
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    );
    app.picking = false;
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| app.background_logic(ctx));
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT]
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Close))
    );
}

#[test]
fn unavailable_tray_falls_back_to_safe_quit_instead_of_hiding_the_window() {
    let temporary = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    let mut app = DesktopApp::from_context(&ctx, temporary.path().to_owned()).unwrap();
    app.preferences.close_to_tray = true;
    app.adding = true;
    let output = ctx.run_logic(&egui::RawInput::default(), |ctx| app.window_close(ctx));
    assert!(app.closing);
    let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
    );
    assert!(
        !commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::Visible(false)))
    );
}

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
