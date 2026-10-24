# Browser integration tests

All browser test sources and the shared filename corpus live here.

```powershell
.\build.cmd test -p rustdm-browser-host --locked
python -m unittest discover -s tests/browser -p "test_*.py"
```

Node 24+ runs the extension tests without installing npm packages. For the full
bridge/host/CLI HTTP test, build binaries and supply their absolute paths:

```powershell
.\build.cmd build --workspace --release --locked
$env:RUSTDM_NATIVE_HOST = "$PWD\target\release\rustdm-browser-host.exe"
$env:RUSTDM_CLI = "$PWD\target\release\rustdm.exe"
node --test tests/browser/extension.test.mjs
```

On Linux, set the same environment variables to absolute executable paths without
`.exe`. CI runs the Rust, Python, and full Node suites on Windows and Linux.
Without those variables, only the subprocess HTTP case is skipped. Rust tests
launch the real host with framed input and exercise Chromium/Firefox caller
formats, invalid framing/schema/paths, duplicate jobs, and destination preservation.

The Node integration case runs the real extension bridge module using a framed
process transport in place of the browser API, then downloads byte-exact fixture
data from the resulting queue using the real CLI. It uses fresh temporary data;
no public server, browser profile, or registration is required.

## Optional real Edge smoke test

```powershell
python extensions/browser/build_extension.py
$env:RUSTDM_EDGE = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
node tests/browser/edge_smoke.mjs
```

This launches Edge headlessly in a fresh ignored `target/browser-smoke` profile,
checks that the actual extension service worker loads, seeds a selected-link
draft through the browser API, and validates the real review page and its
modules. It captures the review page without using your normal browser profile.

With a host registered against a dedicated test queue, setting
`RUSTDM_SMOKE_SUBMIT=1` additionally submits the real review form and checks for a
native queue response. This mode creates a pending example-file job; use a fresh
isolated queue and remove only your temporary registration afterward. It is not
run in headless CI. The build was exercised locally in Edge with temporary HKCU
registration, then that registration was removed. Firefox manifests and caller
arguments have automated coverage; its native UI still needs a browser check.
