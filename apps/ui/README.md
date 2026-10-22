# RustDM desktop

A native Rust desktop preview built with egui/eframe. A dark slate dashboard,
blue accents, persistent download cards, queue controls, search, and attempt
history share the existing `rustdm-core` engine with the CLI.

## Run

```powershell
.\build.cmd build -p rustdm-ui --release --locked
.\target\release\rustdm-ui.exe
```

Or run from source: `cargo run -p rustdm-ui --locked`. On Linux, install
`libxkbcommon-dev` and `libwayland-dev` for builds, and use a desktop session with
OpenGL support. The current published v0.1.0 archives contain the CLI; this
new desktop preview is built from source and is not published as a release yet.

## Download

1. Click **New download** (or press Ctrl+N) and paste a direct HTTP/HTTPS file URL.
2. Click **Browse** to choose a full destination filename, or type the path.
3. Expand **Transfer options** for connections, a KiB/s limit, an expected SHA-256,
   or resuming an existing partial download at that same output path.
4. Choose **Add to queue** or **Download now**. Download now starts the queue,
   including any jobs already ahead of the new download.
5. **Pause queue** saves the active partial download and stops later jobs.
   **Start queue** continues pending/paused jobs. **Retry** requeues a failed job;
   start the queue to execute it. **History** shows errors and terminal attempts.

Jobs execute sequentially; each job uses parallel range connections. Pause/resume
has the same server-validator requirements as the CLI. A checksum mismatch keeps
the destination absent. An already complete destination is never overwritten.
Closing an active window requests pause and waits for the worker to stop before
exiting. Partial files are retained beside the destination.

## Persistence

The default queue is `%LOCALAPPDATA%\RustDM` on Windows, `$XDG_DATA_HOME/RustDM`
when set on Linux, or `~/.local/share/RustDM` otherwise. The sidebar's storage
label shows its full location on hover. Choose another directory with:

```powershell
.\target\release\rustdm-ui.exe --state-dir "D:\RustDMData"
.\target\release\rustdm.exe --state-dir "D:\RustDMData" queue list
```

Both interfaces can inspect the same queue; the core's runner lock allows one
executor at a time. Headers and embedded URL credentials cannot be queued.
URLs/query parameters are persisted, as documented in [queue semantics](../../docs/QUEUE.md).

The native file picker and queue transactions run on background workers. A
Tokio runtime owns download tasks, and a watch channel coalesces progress rather
than accumulating a message per chunk. Lifecycle events trigger fresh snapshots.
Views, form validation, theme, and background integration are separate modules.
All desktop tests live under repository-root `tests/ui/`.

Tray behavior, per-job scheduling, remembered window preferences, installer,
macOS validation, and desktop release archives remain future work. The UI does
not add a second transfer engine or browser capture.
