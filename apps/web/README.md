# Web interface

Reserved for RustDM's future local web dashboard and management API. No web server
or frontend is implemented in this milestone.

The server adapter will use `crates/core`; the frontend will consume its API for
queue state, progress, and download actions. Authentication and explicit access
controls must be designed before exposing management beyond the local computer.

See [the roadmap](../../ROADMAP.md) and [architecture](../../docs/ARCHITECTURE.md).
