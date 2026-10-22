# RustDM CLI

The `rustdm` executable is the current working interface. It depends on the
shared [core crate](../../crates/core), parses arguments, handles Ctrl+C, and
renders progress, transfer speed, and estimated time remaining.

From the repository root:

```sh
cargo run -p rustdm -- --help
cargo run -p rustdm -- "https://example.com/file.zip" -o file.zip -c 4
```

See [the project README](../../README.md) for resume behavior and limitations.
