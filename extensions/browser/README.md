# RustDM browser bridge

An initial integration for public direct HTTP/HTTPS files. Right-click a link,
choose **Queue link in RustDM...**, review the URL and filename, then click
**Add to RustDM queue**. The toolbar button also opens a form for a pasted URL.
Open RustDM and start the queue to download. If its queue is already running,
new jobs can execute after the current job.

The bridge uses our own Rust native messaging host and the shared core queue.
It does not intercept downloads, read cookies, capture page requests, handle
YouTube watch pages, or assemble media streams. Referrer/cookie/header handoff
is a separate milestone because those fields cannot currently be queued.

## Build

From the repository root, with Python 3.12+ and the Rust toolchain:

```powershell
.\build.cmd build --workspace --release --locked
python extensions/browser/build_extension.py
```

Unpacked extensions are generated in `dist/browser-extensions/chromium` and
`dist/browser-extensions/firefox`. No npm install is needed. Browser preview
archives include both extensions, the native executable, and setup files.

## Windows setup

1. Open `edge://extensions` or `chrome://extensions`, enable Developer mode, and
   **Load unpacked** from `dist/browser-extensions/chromium`. Its embedded public
   key gives development builds ID `ipnpmnioeedgmnhlfameghfgaemfnmjm`.
2. Preview the current-user host registration:

   ```powershell
   python extensions/browser/install_host.py --browser edge --host target/release/rustdm-browser-host.exe --download-dir "D:\Downloads" --dry-run
   ```

3. Run the same command with `--register` in place of `--dry-run`.
   Use `--browser chrome` for Chrome. Setup copies the executable into
   `%LOCALAPPDATA%\RustDM\browser-host\BROWSER`, creates `browser-host.json`, and
   registers `com.rustdm.bridge` under that browser's **HKCU** native messaging
   key. Administrator access is not needed. Unrelated existing registrations
   and manifests are preserved. Re-run setup to update our installed host.
4. Right-click a file link, review it, and queue it. Files go inside the locally
   configured download folder. The default queue matches the desktop's
   `%LOCALAPPDATA%\RustDM`.

Use `--state-dir "D:\RustDMData"` if the desktop uses that custom directory.
Register separately for each browser; each host config points to the same queue
by default. Download folders are created during registration if needed. Setup
does not load extensions into your profile.

For Firefox 140+, open `about:debugging#/runtime/this-firefox`, choose **Load
Temporary Add-on**, and select the generated Firefox `manifest.json`. Register
with `--browser firefox`. Its ID is `rustdm@mahtab04`. Temporary add-ons disappear
on browser restart; a signed store release is future work.

In an extracted preview archive, pass the extracted `rustdm-browser-host.exe` as
`--host` and load `browser-extensions/chromium` or `firefox` directly.

## Linux setup

Build using `cargo` and `python3`. Run the same setup script with
`--host target/release/rustdm-browser-host`, an absolute download folder, and
`--browser chrome`, `chromium`, `edge`, or `firefox`. Preview with `--dry-run`,
then use `--register`. It writes the standard per-user native manifest under
your home directory. Use the generated browser extension as described above.

Standard non-sandboxed browsers and default profile locations are targeted.
Snap/Flatpak, custom browser config roots, macOS registration, signed/store
packages, and automatic desktop launching are not implemented.

## Privacy and behavior

- Permissions are only `contextMenus`, `nativeMessaging`, and `storage`. There
  are no content scripts, host permissions, cookies, or web-request hooks.
- Selected URLs temporarily live in session storage, not in review tab addresses.
  Review pages consume drafts once; drafts expire after ten minutes. Nothing is
  sent to the native host until Submit.
- The host accepts configured browser identities and bounded, versioned messages.
  It accepts a basename rather than an arbitrary output path and preserves
  existing files and queued destinations.
- URLs and query parameters persist in the queue, as described in
  [queue semantics](../../docs/QUEUE.md). Browser session secrets are not copied.
- Success means **queued**, not downloaded. Inspect desktop history for HTTP
  errors. If the connection fails after submission, inspect the queue before
  retrying: the enqueue may already have committed.
- The public Chromium key fixes unpacked development identity. It is not a
  private signing key or authentication secret.

## Troubleshooting and removal

For "native host not found", check the installed manifest, absolute executable
path, browser registration key, and extension ID. For a missing queued job, check
both applications' state directories. For collisions, choose another filename.
Authenticated links can fail because cookies/referrer headers are not forwarded.

Disable integration by removing the extension. To unregister on Windows, remove
only the HKCU key for `com.rustdm.bridge` printed by setup. On Linux, remove only
the native manifest printed by setup. The copied host installation folder can
then be removed separately; retain queue/download data.

See the [wire protocol](../../docs/BROWSER.md),
[browser tests](../../tests/browser/README.md), and [roadmap](../../ROADMAP.md).
