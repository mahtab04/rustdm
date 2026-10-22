# Queue and history

The shared `DownloadQueue` stores jobs and finished attempts. `QueueRunner`
executes pending/paused jobs in order through the existing `DownloadManager`.
The CLI, future UI, and web adapter can use these same application services.

## Commands

```sh
rustdm --state-dir ./jobs queue add "https://example.com/file.zip" -o ./downloads/file.zip -c 4
rustdm --state-dir ./jobs queue list
rustdm --state-dir ./jobs queue run
rustdm --state-dir ./jobs history --json
rustdm --state-dir ./jobs queue retry 1
rustdm --state-dir ./jobs queue run
```

`--state-dir` is global and can precede the command or follow its options. It
defaults to `.rustdm` in the current directory. Output paths become absolute at
enqueue time; the queued URL and destination are immutable. IDs increase as jobs
are added. Duplicate unfinished destinations are rejected. Retry the existing
job or choose a different destination.

`queue add` accepts connection/retry settings, `--limit-rate BYTES_PER_SECOND`,
and `--resume` for an existing
partial download. Subsequent attempts automatically request resume when saved
download metadata exists; the manager still checks remote identity before reuse.
Servers without usable ranges/validators cannot resume their partial data and
need a new destination for a fresh download.

A bandwidth limit is a positive integer in bytes per second, shared across all
connections of that job. For example, `queue add URL -o FILE --limit-rate 1048576`
sets 1 MiB/s. It is persisted as optional `bandwidth_limit` in job snapshots and
reused on retry/resume. Missing limits in existing version-1 records mean
unlimited speed. Zero or invalid saved limits are rejected without rewriting the
snapshot. Jobs execute sequentially, so each job uses its own configured cap.
There is currently no command to edit a queued limit.

## Lifecycle

| Current state | Trigger | Result |
|---|---|---|
| Pending | `queue run` claims the job | Running; new attempt saved |
| Running | Successful final-file publication | Completed; byte counts saved |
| Running | Download failure | Failed; error saved; later jobs continue |
| Running | Ctrl+C | Paused; later jobs stay pending |
| Running after an abandoned session | Next runner acquires the exclusive lock | Prior attempt recorded as paused |
| Paused | `queue run` | New running attempt; validated partial reuse |
| Failed or paused | `queue retry ID` | Pending; prior attempt history retained |
| Completed | Any later `queue run` | Skipped; destination preserved |

Queue execution is sequential, while each job can use parallel HTTP connections.
Only one runner may execute a particular queue at a time. `queue list`, `history`,
and additions remain available during execution because record transactions hold
a different, short-lived lock. Retry is refused while a runner is active.

## Persistence and recovery

The state directory contains:

- `queue.json`: versioned snapshot with immutable job fields, state, and attempts.
- `queue.lock`: OS lock protecting short read/modify/write transactions.
- `runner.lock`: OS lock held throughout one queue execution session.

Each update is serialized, written to a temporary file in the same directory,
synced, and atomically published. Parse/version failures preserve the original
snapshot and return an error. Lock files remain on disk; the lock is released by
the OS when the corresponding handle or process ends.

Download part files remain beside their individual output, using the existing
`.rustdm`/legacy `.rustidm` layout. The queue does not delete partial files on
failure or retry. A replacement runner records unfinished running attempts as
paused, then can resume them using the normal manager checks.

Output publication and the final queue update are separate operations. If a
process stops between them, an output may already be complete while history
still records interruption. The queue preserves that output and reports an
existing-file error on another attempt; it does not infer success from a filename.

## History and automation

`history` lists terminal attempts for queued jobs only. `history --json` includes
job ID, attempt number, source URL, absolute output path, state, UTC timestamps as
Unix seconds, completed byte counts, resumed bytes, and error details. Retrying a
job appends a new attempt; it does not replace earlier failures or pauses.
`queue list --json` includes current job snapshots and their attempt records.

An empty queue or successful run exits 0. A run with failed jobs or cancellation
exits nonzero after recording the results. CLI syntax errors are reported by Clap.

Headers and URL userinfo are rejected at enqueue time. The initial queued-request
model supports GET downloads without persisted browser credentials. Direct
downloads still support `-H` headers and are not recorded in queue history.
Queue snapshots do contain source URLs and query parameters. There is no job
editing, pruning, scheduling, or parallel-job execution in this milestone.
