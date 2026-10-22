# Contributing

Develop one milestone at a time as described in [ROADMAP.md](ROADMAP.md).
Keep download behavior in the library; CLI, desktop, and browser adapters should
call the same application service.

## Development checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

On Windows, run these from a Visual Studio Developer Command Prompt, or use
`build.cmd test --locked` and `build.cmd clippy --all-targets --locked`. The helper
discovers installed C++ tools with vswhere; Rust/Cargo must be on PATH.

The repository is a Cargo workspace. Shared behavior belongs in `crates/core`;
terminal behavior belongs in `apps/cli`. `apps/ui`, `apps/web`, and
`extensions/browser` are reserved for later milestones. Use `cargo run -p rustdm`
to select the current CLI application.

Keep test source files and fixtures under repository-root `tests/`, grouped by
component. See [test organization and Cargo wiring](tests/README.md) before adding
a suite. Unit tests retain private access through test-only module paths;
integration suites are explicit targets in the owning package manifest.

Tests use an actual local HTTP server with known binary data. Add behavioral
coverage for response validation, resumed state, cancellation, and output safety
when changing those paths. Do not depend on public download servers in tests.

## Release packages

CI packages and smoke-tests the native executable after workspace validation.
Packaging uses Python 3.12+ and the standard library, without Python dependencies.
See [the release checklist](docs/RELEASE.md) for local commands, versioned notes,
archive checksums, and the tag-triggered draft-release workflow. `dist/` and Python
bytecode caches are ignored. Publishing a draft is a separate release step.

## Git workflow

Use a feature branch for contributions and a focused pull request. Keep changes
small enough to review, explain the resulting behavior, and include validation.

Use Conventional Commits:

```text
feat(cli): add a download queue command
fix(engine): reject changed range validators
refactor(core): encapsulate saved download state
test(http): cover truncated response recovery
docs: document browser integration milestone
ci: check Windows and Linux builds
```

Update README and CHANGELOG for user-visible changes. Update architecture docs
when responsibility boundaries change. Commit Cargo.lock with dependency changes.
Push coherent, tested commits; do not force-push shared history.
