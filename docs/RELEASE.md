# Release and preview packaging

The CLI currently reports version **0.1.0**. The first public release was
published on 2026-10-22: [RustDM v0.1.0](https://github.com/mahtab04/rustdm/releases/tag/v0.1.0).
Windows and Linux package checks passed before publication; archive and binary
hashes and source-commit metadata were independently verified.
The CLI milestone remains active while the roadmap has unfinished engine work.

## Packages

| Target | Package |
|---|---|
| Windows x86-64 MSVC | `rustdm-vVERSION-x86_64-pc-windows-msvc.zip` |
| Linux x86-64 GNU | `rustdm-vVERSION-x86_64-unknown-linux-gnu.tar.gz` |

Each archive contains the executable, README, changelog, usage/architecture
documents, versioned release notes, and `BUILD.json`. Build metadata identifies
the source commit, target, Rust compiler, source dirty status, and executable
SHA-256. A neighboring `.sha256` file identifies the archive hash. Linux packages
use the runner's GNU/glibc environment; they are not static or musl builds.
There are no macOS/ARM packages or installer in this initial workflow.

CI builds and tests Windows and Linux before packaging. It extracts each archive,
verifies the archive and executable hashes, and runs the extracted executable's
version/help commands. Successful branch builds retain packages as Actions
artifacts for 14 days. Pull requests validate packaging without uploading assets.

## Desktop previews

Current branch CI also produces `rustdm-desktop-vVERSION-TARGET.zip` (Windows)
or `.tar.gz` (Linux) plus `.sha256`. Each contains `rustdm-ui`, `apps/ui/README.md`, the
shared documentation, and build metadata with application/executable identity.
The checker validates every native archive present, supporting both the CLI and
desktop while remaining compatible with older CLI metadata. Desktop smoke checks
use `--version` and `--help` without opening a display; CLI checks also exercise
`download --help`. GUI interactions require a desktop session and manual checks.

In a successful Actions run, download `rustdm-TARGET`, unzip that artifact, then
extract its `rustdm-desktop-...` archive. On Linux, launch it in an OpenGL desktop
session. These are development previews identified by commit in `BUILD.json`;
they do not change the published v0.1.0 CLI assets. Ordinary main pushes only
build/test/upload artifacts. Version-tag releases still create drafts and require
separate publication.

## Local packaging

Python 3.12+ and the native Rust toolchain are required. Build and package from
the repository root on Windows:

```powershell
.\build.cmd build --workspace --release --locked
python .github/scripts/package_release.py --target x86_64-pc-windows-msvc --binary target/release/rustdm.exe
python .github/scripts/package_release.py --app desktop --target x86_64-pc-windows-msvc --binary target/release/rustdm-ui.exe
python .github/scripts/check_package.py dist
```

On Linux:

```sh
cargo build --workspace --release --locked
python3 .github/scripts/package_release.py --target x86_64-unknown-linux-gnu --binary target/release/rustdm
python3 .github/scripts/package_release.py --app desktop --target x86_64-unknown-linux-gnu --binary target/release/rustdm-ui
python3 .github/scripts/check_package.py dist
```

Output goes to ignored `dist/`; use `--output ANOTHER_DIRECTORY` for another run.
Existing packages are never overwritten. The packager checks the executable
version against the workspace and requires a matching native host. It does not
perform cross compilation. Untagged local packages may have `source_dirty: true`
and are development previews. `--tag vVERSION` additionally requires a clean
checkout and a tag matching the workspace version. It validates the tag value;
the release workflow separately requires the GitHub tag to exist.

## Release checklist

1. Choose a version and update `[workspace.package].version` in `Cargo.toml`.
   Run Cargo to refresh package versions in `Cargo.lock` if changed.
2. Add `docs/releases/vVERSION.md`, describing implemented behavior, remaining
   gaps, and any partial-download/queue compatibility changes. Update the
   changelog and roadmap to distinguish preparation from a published release.
3. Run formatting, strict Clippy, all workspace tests, and the release build as
   documented in [CONTRIBUTING.md](../CONTRIBUTING.md). Push the tested commit.
4. Check the Windows and Linux CI results. Download and try each native package
   on the corresponding operating system, including one real HTTPS download and
   pause/resume. Confirm optional SHA-256 and bandwidth-limit behavior.
5. Confirm the exact commit and version to release. Create and push its version
   tag, for example:

   ```sh
   git tag -a v0.1.0 -m "RustDM v0.1.0"
   git push origin v0.1.0
   ```

6. The **Release** workflow reruns CI and packaging on that tag. A mismatched
   tag/version or dirty checkout fails packaging. After both platforms pass,
   the workflow creates a **draft** GitHub release with the archives and their
   checksum files, using the versioned notes. Existing releases are not replaced.
7. Review the draft's source tag, notes, assets, and checksums. Publish it in
   GitHub Releases when it is ready; publishing remains a separate action.
8. Record the published release link/date in the changelog and roadmap. Start
   the next version's notes when development continues.

Running **Release** manually from a branch builds preview artifacts only; it
does not create a release or a tag. A failed release can be investigated through
the job logs; fix source in a new commit rather than moving a published tag.

## Verify a downloaded archive

Compare the SHA-256 file with PowerShell:

```powershell
Get-FileHash .\rustdm-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256
Get-Content .\rustdm-v0.1.0-x86_64-pc-windows-msvc.zip.sha256
```

On Linux, place the archive and its checksum file together:

```sh
sha256sum --check rustdm-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

After extracting, run `rustdm --version` and `rustdm download --help`.
For downloads, `-o` requires a full destination filename, for example
`rustdm download "https://curl.se/ca/cacert.pem" -o "cacert.pem"`.
