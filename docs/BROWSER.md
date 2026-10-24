# Browser native messaging protocol

Host: `com.rustdm.bridge`. Executable: `rustdm-browser-host`. The adapter follows
[Chromium native messaging](https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging)
and [Firefox native messaging](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging).
Generated manifests use each browser's supported
[background environment](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/background).

## Framing

stdin/stdout carry UTF-8 JSON prefixed by an unsigned 32-bit **native-endian** byte
length (little-endian on supported x86-64 targets). Both bodies are limited to
65,536 bytes. Zero-length, oversized, and truncated frames fail the connection.
stdout carries only framed responses in protocol mode; diagnostics go to stderr.
Rust stdio writes raw bytes without CRT text-mode newline translation.

The host processes frames until EOF. Invalid JSON or semantic requests return an
error frame; later valid requests can still succeed. Truncated framing exits
nonzero rather than resynchronizing. `--version` and `--help` are separate
maintenance modes, not native messaging connections.

## Version 1

```json
{"version":1,"id":"request-uuid","action":"enqueue","url":"https://example.com/file.zip","filename":"file.zip"}
```

Only these fields and action are accepted. IDs are nonempty and at most 80 UTF-8
bytes. URLs are at most 16 KiB and must be HTTP/HTTPS without embedded credentials.
Filenames are 1 to 180 UTF-8 bytes, a single basename, without control characters,
Windows reserved names, path/drive/alternate-stream syntax, or trailing dots/spaces.

Success:

```json
{"version":1,"id":"request-uuid","ok":true,"job_id":42,"output":"D:\\Downloads\\file.zip"}
```

Error:

```json
{"version":1,"id":"request-uuid","ok":false,"error":"Destination already exists; choose another filename"}
```

Schema-invalid requests may return `id: null`. Clients correlate responses by ID.
A lost response is ambiguous: the transaction may have committed. Duplicate
output paths are rejected; IDs are not persisted idempotency keys. There is no
automatic retry after ambiguous IO.

## Local configuration and caller checks

`browser-host.json` lives beside the installed executable:

```json
{
  "version": 1,
  "state_dir": "D:\\RustDMData",
  "download_directory": "D:\\Downloads",
  "allowed_callers": ["chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/"]
}
```

Paths must be absolute; the download folder must exist when the host runs. The
browser cannot change them. Chromium supplies its extension origin and optionally
a numeric `--parent-window` argument. Firefox supplies a native manifest path and
add-on ID; the supported ID is `rustdm@mahtab04`. The browser manifest and host
configuration both restrict caller identity. There is no shell, network listener,
or arbitrary command-execution action.

For tests, `rustdm-browser-host --config FILE CALLER` overrides the configuration
path. Browser manifests use the absolute executable path without shell wrappers.

`Host` validates requests and composes `DownloadQueue::enqueue`. It starts no
executor or desktop process. The desktop polls the shared queue; CLI users select
it with `--state-dir`. This first bridge does not persist headers, referrers,
cookies, or page titles. Queue URLs and query parameters remain persisted.

## Validation

Tests live under `tests/browser`. Rust tests run the actual host process, checking
framing, callers, schema, filenames, multiple messages, and file/queue collisions.
JavaScript tests cover drafts, explicit submission, and response correlation.
An end-to-end test runs the extension's real bridge module, replaces only the
browser transport with framed stdin/stdout, queues through the actual host, then
downloads known HTTP bytes with the CLI. Python tests check generated manifests
and side-effect-free setup plans. This is not a browser-profile UI test.

See [test commands](../tests/browser/README.md). Native browser loading and menu
UI also need validation in the intended browser; protocol tests do not establish
store-signing compatibility.

The optional `edge_smoke.mjs` driver was also run against a real isolated Edge
profile: the extension loaded, the review page consumed its draft, and a form
submission reached a temporarily registered native host and created a pending
job. That test registration was removed afterward. Firefox browser UI validation
remains outstanding; simulated native arguments and generated manifests are tested.
