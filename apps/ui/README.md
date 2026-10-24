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
desktop preview can be built from source or downloaded from a successful
[CI run's artifacts](https://github.com/mahtab04/rustdm/actions/workflows/ci.yml).
Extract `rustdm-desktop-vVERSION-TARGET` and run `rustdm-ui.exe` / `rustdm-ui`.
These portable preview packages are not a new published release or an installer.

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
By default, closing an active window requests pause and waits for the worker to stop before
exiting. Pending additions finish saving without starting another transfer while
closing. If a native file picker is open, close it to finish quitting. Controls
are disabled during shutdown. Unexpected download-task failures appear in the
dashboard, and startup failures show a native error dialog.
Partial files are retained beside the destination.

## Windows system tray

On Windows, RustDM creates a tray icon while the app runs. Click **Hide to tray**
to keep downloads running in the background. Left-click the icon to restore the
window, or right-click for **Show RustDM**, **Start queue**, **Pause queue**, and
**Quit RustDM**. Windows may place the icon in its hidden-icons overflow.

In **Settings**, enable **Close window to system tray (keep downloading)** to
make the window's close button hide it. This is off by default, including for
older preference files. The setting takes effect when saved. Hiding does not
pause transfers or stop pending additions. Finish an open native destination
picker before hiding the window.

**Quit RustDM** always pauses the active transfer, waits for pending additions
and file-picker replies, and exits, even with close-to-tray enabled. Open pickers
must be closed to finish quitting. Start/pause menu actions reflect queue state;
new starts are ignored once shutdown begins. The tooltip shows ready,
downloading, or saving progress.

Tray support currently targets Windows. Linux keeps normal pause-and-quit close
behavior and disables the close-to-tray setting. If Windows tray creation fails,
the dashboard shows a warning and uses safe quit instead of hiding the window.
There is no automatic startup, hidden launch, or second download process.

## Connection details

Expand **Connection details** under a download card to see connection number,
downloaded bytes, and live status (sending request, receiving data, retrying,
completed, paused, or failed). The panel opens automatically for active jobs.

Rows represent logical transfer workers, not physical TCP sockets: the HTTP
client can reuse connections and a worker may handle multiple saved ranges.
Counters show new response-body bytes written during this execution, excluding
saved resume bytes, probe traffic, and protocol headers. A restarted single
stream can count retransferred bytes again. A completed worker has finished its
body transfer; the job still needs assembly/checksum/publication to succeed.

Telemetry is live for jobs started in this window and retained until it closes.
It is not persisted or replayed for jobs run by another CLI/desktop process.
Queue history continues to store terminal job attempts.

## Settings

Open **Settings** in the sidebar to save the default connection count (1?16),
speed limit in KiB/s (blank means unlimited), resume behavior, and an optional
existing absolute folder for the native file picker. Save settings explicitly;
Cancel discards edits. A form already open keeps its edits. New forms use the
saved defaults; queued and running jobs retain their original options.

Preferences live in `preferences.json` beside the queue. Writes use a separate
OS lock and an atomic replacement. Missing files use defaults. Malformed,
unsupported-version, or invalid files produce a warning and remain untouched;
settings cannot be saved over them. A save already in progress in another window
returns a retry message rather than blocking the UI. Repair or rename the file before saving
again. An unavailable saved folder falls back to the file picker's default.
URLs, output filenames, and expected checksums are not stored as preferences.

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

Linux/macOS tray support, per-job scheduling, remembered window preferences, installer,
and macOS validation remain future work. Browser capture is a later milestone.
