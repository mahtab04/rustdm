# Desktop tests

Unit suites are wired from the desktop's owning modules using test-only paths.
`model.rs` covers destination, speed-limit, checksum, and filter behavior.
`backend.rs` covers actual HTTP execution through the desktop adapter and
persistent queue, plus queue error delivery. No display server is required.

```sh
cargo test -p rustdm-ui --locked
```

See [all tests](../README.md) and [desktop usage](../../apps/ui/README.md).
