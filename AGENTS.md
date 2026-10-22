# Project working agreement

- Build RustDM incrementally: finish the CLI milestone, then desktop UI, then
  browser integration and a web interface. Keep the current milestone in ROADMAP.md.
- Use idiomatic Rust: encapsulated structs, methods, traits at real extension
  points, composition, and ownership. Keep user interfaces out of the core engine.
- Keep shared code in crates/core, CLI code in apps/cli, future desktop code in
  apps/ui, web code in apps/web, and browser adapters in extensions/browser.
- Keep all test source files and fixtures under repository-root tests/, grouped
  by component. Wire unit suites through test-only module paths and integration
  suites through explicit Cargo test targets; preserve private API boundaries.
- Keep modules focused; avoid inheritance emulation, speculative abstractions,
  and duplicate download implementations for different interfaces.
- Run workspace-wide formatting, Clippy with warnings denied, and relevant tests
  before a push. Keep Cargo.lock and shared dependencies at the repository root.
- Use Conventional Commits with one coherent change per commit. Update README,
  architecture documentation, and the changelog when behavior/interfaces change.
- The user has requested ongoing commits and pushes for this project. Push tested
  logical changes to the configured origin; never force-push or rewrite published
  history without explicit authorization. Do not commit unrelated workspace work.
- Keep credentials, Cargo caches, build outputs, partial downloads, vendor
  binaries, extracted proprietary code, and analysis artifacts out of this repo.
- When Windows sandbox restrictions interfere with loopback HTTP tests, report
  the restriction and run the same tests with the appropriate execution access.
- Do not claim a full IDM implementation: document the current scope and gaps.
