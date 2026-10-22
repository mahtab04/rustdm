# RustDM CLI

The `rustdm` executable is the current working interface. It depends on the
shared [core crate](../../crates/core), parses arguments, handles Ctrl+C, and
renders progress, transfer speed, and estimated time remaining.

From the repository root:

```sh
cargo run -p rustdm -- --help
cargo run -p rustdm -- "https://example.com/file.zip" -o file.zip -c 4
cargo run -p rustdm -- queue add "https://example.com/file.zip" -o file.zip
cargo run -p rustdm -- queue run
cargo run -p rustdm -- history --json
```

See [the project README](../../README.md) for resume behavior and limitations.
See [queue commands](../../docs/QUEUE.md) for persistent jobs and history.
