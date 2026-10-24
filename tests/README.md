# Tests

All test source files live in this repository-level folder, grouped by the
component they exercise.

| Folder | Coverage |
|---|---|
| `core/checksum.rs` | Digest validation, serialization, and known SHA-256 vectors |
| `core/segments.rs` | Bounded range planning, full byte coverage, and large-file boundaries |
| `core/metrics.rs` | Transfer speed and ETA calculations |
| `core/queue/` | Queue persistence, locking, recovery, and history |
| `cli/args.rs` | Argument parsing, rate validation, and command compatibility |
| `cli/downloads.rs` | HTTP transfers, connection telemetry/retry/resume accounting, dynamic scheduling/resume, validator interoperability, checksum/publication safety, aggregate pacing, cancellation/resume, queue execution, and CLI subprocesses |
| `ui/` | Desktop form validation, filtering, background HTTP execution, queue/worker errors, preferences round-trip/corruption safety, and hidden tray command/HTTP/shutdown handling |
| `web/` | Reserved for web interface tests |
| `browser/` | Native framing/callers, extension validation/drafts, real host/CLI HTTP bridge, setup plans, and optional real Edge smoke |

Run every implemented suite from the repository root:

```sh
cargo test --workspace --locked
```

Run individual suites or filter by test name:

```sh
cargo test -p rustdm-core queue::tests --locked
cargo test -p rustdm-core metrics::tests --locked
cargo test -p rustdm args::tests --locked
cargo test -p rustdm --test downloads --locked
```

On Windows, use a Visual Studio Developer Command Prompt or replace `cargo`
with `.\build.cmd`.

The source location is centralized; Rust's unit and integration test roles are
preserved. Unit suites are loaded through `#[cfg(test)]` and `#[path]` from
their owning modules so they can verify private behavior without exposing it
as public API. The CLI manifest registers the HTTP suite with `[[test]]` so
Cargo builds the CLI executable and supplies `CARGO_BIN_EXE_rustdm`.

Keep new test implementations here. Add a test-only module reference for private
unit tests, or an explicit `[[test]]` target in the owning package for integration
tests. The root manifest is a virtual workspace, so it does not automatically
discover files in this folder. Formatting, Clippy, and workspace tests follow
the registered modules and targets. Fixtures should also live under `tests/`.
Tests use local HTTP fixtures and temporary directories, with no public server
dependency. Desktop and native-host Rust suites are registered. See [browser tests](browser/README.md)
for Node/Python/Edge commands; web suites remain reserved.

For the native Windows tray, build the desktop release and run this from an
interactive Windows desktop (it opens a test window and operates its tray menu):

```powershell
.\build.cmd build -p rustdm-ui --release --locked
powershell -NoProfile -ExecutionPolicy Bypass -File tests/ui/windows_tray_smoke.ps1
```

The smoke script uses its own fresh state directory under ignored `target/`,
checks close-to-tray, restoration by click/menu, hidden Quit, and native-window
cleanup, and captures only RustDM's tray menu. It simulates the notification
messages delivered by Windows to tray-icon 0.26, including overflow icons. Use
`-Binary PATH` or `-StateDirectory NEW_PATH` to override defaults. It does not run
in headless CI; cross-platform Rust tests cover background command behavior.
