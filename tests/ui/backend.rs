use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

#[test]
fn desktop_backend_downloads_through_the_persistent_queue_and_reports_completion() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/file", listener.local_addr().unwrap());
    let peer = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut requests = 0;
        while requests < 2 && Instant::now() < deadline {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc");
            requests += 1;
        }
        assert_eq!(requests, 2);
    });
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("desktop.bin");
    let mut backend = Backend::new(temp.path().join("jobs"), egui::Context::default()).unwrap();
    backend.add(
        DownloadOptions::new(url, &output).with_sha256(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                .parse()
                .unwrap(),
        ),
        true,
    );
    assert!(matches!(
        backend.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Added(Ok(true))
    ));
    backend.start();
    loop {
        if let Event::Stopped(result) = backend
            .events
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
        {
            assert_eq!(result.unwrap().completed, 1);
            break;
        }
    }
    assert_eq!(std::fs::read(output).unwrap(), b"abc");
    let history = backend.queue.history().unwrap();
    assert_eq!(history[0].state, rustdm_core::JobState::Completed);
    peer.join().unwrap();
}

#[test]
fn desktop_backend_returns_queue_errors_as_events_without_creating_jobs() {
    let temp = tempfile::tempdir().unwrap();
    let backend = Backend::new(temp.path().join("jobs"), egui::Context::default()).unwrap();
    backend.add(
        DownloadOptions::new("ftp://example.test/file", temp.path().join("bad.bin")),
        false,
    );
    assert!(matches!(
        backend.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        Event::Added(Err(_))
    ));
    assert!(backend.queue.jobs().unwrap().is_empty());
}
