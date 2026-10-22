# Tests

All test source files live in this repository-level folder, grouped by the
component they exercise.

| Folder | Coverage |
|---|---|
| `core/metrics.rs` | Transfer speed and ETA calculations |
| `core/queue/` | Queue persistence, locking, recovery, and history |
| `cli/args.rs` | Argument parsing, rate validation, and command compatibility |
| `cli/downloads.rs` | HTTP transfers, aggregate pacing, cancellation/resume, queue execution, and CLI subprocesses |
| `ui/` | Reserved for desktop UI tests |
| `web/` | Reserved for web interface tests |
| `browser/` | Reserved for browser integration tests |

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
dependency. UI, web, and browser suites will be wired in when those apps exist.
