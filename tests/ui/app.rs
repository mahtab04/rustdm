use super::*;
use rustdm_core::DownloadQueue;

#[test]
fn right_click_delete_opens_confirmation_before_deleting_the_selected_row() {
    fn frame(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1180.0, 780.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.background_logic(ui.ctx());
                app.dashboard(ui);
                app.delete_dialog(ui.ctx());
            },
        )
    }
    fn point(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("Missing UI label: {label}"))
    }
    fn click(
        app: &mut DesktopApp,
        ctx: &egui::Context,
        position: egui::Pos2,
        button: egui::PointerButton,
    ) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            )
            .drop_without_applying_deltas();
        }
    }
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("queue");
    let queue = DownloadQueue::open(&state).unwrap();
    queue
        .enqueue(rustdm_core::DownloadOptions::new(
            "https://example.test/file",
            temporary.path().join("file.bin"),
        ))
        .unwrap();
    let ctx = egui::Context::default();
    let mut app = DesktopApp::from_context(&ctx, state).unwrap();
    app.jobs = queue.jobs().unwrap();
    frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
    let output = frame(&mut app, &ctx, vec![]);
    let card = point(&output, "file.bin");
    output.drop_without_applying_deltas();
    click(&mut app, &ctx, card, egui::PointerButton::Secondary);
    frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
    let output = frame(&mut app, &ctx, vec![]);
    let delete = point(&output, "Delete files...");
    output.drop_without_applying_deltas();
    click(&mut app, &ctx, delete, egui::PointerButton::Primary);
    assert!(
        app.pending_delete.is_some(),
        "Context menu must request confirmation"
    );
    assert_eq!(queue.jobs().unwrap().len(), 1);
    frame(&mut app, &ctx, vec![]).drop_without_applying_deltas();
    let output = frame(&mut app, &ctx, vec![]);
    let confirm = point(&output, "Delete files");
    output.drop_without_applying_deltas();
    click(&mut app, &ctx, confirm, egui::PointerButton::Primary);
    assert!(app.pending_delete.is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.mutating.is_empty() && Instant::now() < deadline {
        app.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(queue.jobs().unwrap().is_empty());
}

#[test]
fn delete_confirmation_requires_a_button_click_then_dispatches_the_mutation() {
    for label in ["Cancel", "Delete files"] {
        let temporary = tempfile::tempdir().unwrap();
        let state = temporary.path().join("queue");
        let queue = DownloadQueue::open(&state).unwrap();
        let id = queue
            .enqueue(rustdm_core::DownloadOptions::new(
                "https://example.test/file",
                temporary.path().join("file.bin"),
            ))
            .unwrap();
        let ctx = egui::Context::default();
        let mut app = DesktopApp::from_context(&ctx, state).unwrap();
        app.pending_delete = Some(queue.jobs().unwrap().remove(0));
        app.delete_ready = true;
        let draw = |app: &mut DesktopApp, events: Vec<egui::Event>| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(1180.0, 780.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.delete_dialog(ui.ctx()),
            )
        };
        draw(&mut app, vec![]).drop_without_applying_deltas();
        let output = draw(&mut app, vec![]);
        let point = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("Confirmation must show the Delete files button");
        output.drop_without_applying_deltas();
        assert_eq!(
            queue.jobs().unwrap().len(),
            1,
            "Showing confirmation cannot delete a job"
        );
        draw(
            &mut app,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        )
        .drop_without_applying_deltas();
        draw(
            &mut app,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
        .drop_without_applying_deltas();
        assert!(app.pending_delete.is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.mutating.contains(&id) && Instant::now() < deadline {
            app.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            queue.jobs().unwrap().len(),
            if label == "Cancel" { 1 } else { 0 }
        );
    }
}

#[test]
fn closing_waits_for_a_pending_job_mutation_and_handles_its_reply() {
    let temporary = tempfile::tempdir().unwrap();
    let state = temporary.path().join("queue");
    let queue = DownloadQueue::open(&state).unwrap();
    let id = queue
        .enqueue(rustdm_core::DownloadOptions::new(
            "https://example.test/file",
            temporary.path().join("file.bin"),
        ))
        .unwrap();
    let mut app = DesktopApp::from_context(&egui::Context::default(), state).unwrap();
    app.mutating.insert(id);
    app.backend.job_action(id, JobAction::Remove);
    app.begin_close();
    assert!(app.busy(), "Shutdown must drain the pending mutation reply");
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.busy() && Instant::now() < deadline {
        app.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!app.busy());
    assert!(queue.jobs().unwrap().is_empty());
}

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
